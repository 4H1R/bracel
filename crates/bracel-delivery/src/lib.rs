use bracel::{
    axum::http::StatusCode,
    http::error::AppError,
    jobs,
    sea_orm::{ConnectionTrait, DatabaseConnection, DatabaseTransaction, TransactionTrait},
};
use bracel_data::sql;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;
pub mod webhook;

pub const SCHEMA: &str = "CREATE TABLE bracel_notifications(id uuid PRIMARY KEY,scope text NOT NULL,dedupe text NOT NULL,content jsonb NOT NULL,read_at timestamptz,created_at timestamptz NOT NULL DEFAULT clock_timestamp(),UNIQUE(scope,dedupe)); CREATE INDEX bracel_notifications_inbox ON bracel_notifications(scope,created_at,id); CREATE TABLE bracel_notification_preferences(scope text NOT NULL,channel text NOT NULL,enabled boolean NOT NULL,PRIMARY KEY(scope,channel)); CREATE TABLE bracel_mail_delivery(id uuid PRIMARY KEY,scope text NOT NULL,recipient text NOT NULL,sender text NOT NULL,subject text NOT NULL,body text NOT NULL,status text NOT NULL DEFAULT 'pending',accepted_at timestamptz); CREATE TABLE bracel_webhook_inbox(provider text NOT NULL,event_id text NOT NULL,payload jsonb NOT NULL,processed_at timestamptz,received_at timestamptz NOT NULL DEFAULT clock_timestamp(),PRIMARY KEY(provider,event_id));";

pub async fn notify(
    tx: &DatabaseTransaction,
    scope: &str,
    dedupe: &str,
    content: Value,
) -> Result<Uuid, AppError> {
    if scope.is_empty()
        || scope.len() > 2048
        || dedupe.is_empty()
        || dedupe.len() > 200
        || content.to_string().len() > 8192
    {
        return Err(bracel_data::conflict());
    }
    let id = Uuid::now_v7();
    let inserted=tx.execute_raw(sql("INSERT INTO bracel_notifications(id,scope,dedupe,content) VALUES($1,$2,$3,$4::jsonb) ON CONFLICT(scope,dedupe) DO NOTHING",vec![id.into(),scope.into(),dedupe.into(),content.to_string().into()])).await?.rows_affected()==1;
    let row=tx.query_one_raw(sql("SELECT id,content::text AS content FROM bracel_notifications WHERE scope=$1 AND dedupe=$2",vec![scope.into(),dedupe.into()])).await?.ok_or_else(bracel_data::conflict)?;
    if serde_json::from_str::<Value>(&row.try_get::<String>("", "content")?)
        .map_err(|_| bracel_data::conflict())?
        != content
    {
        return Err(bracel_data::conflict());
    }
    let id: Uuid = row.try_get("", "id")?;
    let realtime = tx.query_one_raw(sql("SELECT enabled FROM bracel_notification_preferences WHERE scope=$1 AND channel='realtime'",vec![scope.into()])).await?.map(|row| row.try_get::<bool>("", "enabled")).transpose()?.unwrap_or(true);
    if inserted && realtime {
        bracel_realtime::publish_value(
            tx,
            scope,
            "notifications",
            "notification.created",
            1,
            json!({"id":id}),
        )
        .await?;
    }
    Ok(id)
}
pub async fn inbox(
    db: &impl ConnectionTrait,
    scope: &str,
    before: Option<Uuid>,
    limit: u64,
) -> Result<Vec<Value>, AppError> {
    if !(1..=100).contains(&limit) {
        return Err(bracel_data::conflict());
    }
    db.query_all_raw(sql("SELECT id,content::text AS content,read_at IS NOT NULL AS read FROM bracel_notifications WHERE scope=$1 AND ($2::uuid IS NULL OR id<$2) ORDER BY id DESC LIMIT $3",vec![scope.into(),before.into(),(limit as i64).into()])).await?.into_iter().map(|row|Ok(json!({"id":row.try_get::<Uuid>("","id")?,"content":serde_json::from_str::<Value>(&row.try_get::<String>("","content")?).map_err(|_|bracel_data::conflict())?,"read":row.try_get::<bool>("","read")?}))).collect()
}
pub async fn mark_read(db: &impl ConnectionTrait, scope: &str, id: Uuid) -> Result<(), AppError> {
    if db.execute_raw(sql("UPDATE bracel_notifications SET read_at=COALESCE(read_at,clock_timestamp()) WHERE id=$1 AND scope=$2",vec![id.into(),scope.into()])).await?.rows_affected()!=1{return Err(AppError::new(StatusCode::NOT_FOUND,"Notification not found"));}
    Ok(())
}
pub async fn preference(
    db: &impl ConnectionTrait,
    scope: &str,
    channel: &str,
    enabled: bool,
) -> Result<(), AppError> {
    if !matches!(channel, "mail" | "realtime") {
        return Err(bracel_data::conflict());
    }
    db.execute_raw(sql("INSERT INTO bracel_notification_preferences(scope,channel,enabled) VALUES($1,$2,$3) ON CONFLICT(scope,channel) DO UPDATE SET enabled=excluded.enabled",vec![scope.into(),channel.into(),enabled.into()])).await?;
    Ok(())
}
#[derive(Serialize, Deserialize)]
pub struct MailJob {
    pub delivery_id: Uuid,
}
impl jobs::TypedJob for MailJob {
    const KIND: &'static str = "delivery.mail";
}
pub struct MailIntent<'a> {
    pub scope: &'a str,
    pub dedupe: &'a str,
    pub from: &'a str,
    pub to: &'a str,
    pub subject: &'a str,
    pub text: &'a str,
}
pub async fn enqueue_mail(
    tx: &DatabaseTransaction,
    intent: MailIntent<'_>,
) -> Result<Uuid, AppError> {
    if intent.subject.len() > 200 || intent.text.len() > 16384 {
        return Err(bracel_data::conflict());
    }
    bracel_integrations::mail::message(intent.from, intent.to, intent.subject, intent.text)
        .map_err(|_| bracel_data::conflict())?;
    let id = notify(
        tx,
        intent.scope,
        intent.dedupe,
        json!({"subject":intent.subject}),
    )
    .await?;
    tx.execute_raw(sql("INSERT INTO bracel_mail_delivery(id,scope,recipient,sender,subject,body) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT(id) DO NOTHING",vec![id.into(),intent.scope.into(),intent.to.into(),intent.from.into(),intent.subject.into(),intent.text.into()])).await?;
    let stored = tx
        .query_one_raw(sql(
            "SELECT recipient,sender,subject,body FROM bracel_mail_delivery WHERE id=$1",
            vec![id.into()],
        ))
        .await?
        .ok_or_else(bracel_data::conflict)?;
    for (field, expected) in [
        ("recipient", intent.to),
        ("sender", intent.from),
        ("subject", intent.subject),
        ("body", intent.text),
    ] {
        if stored.try_get::<String>("", field)? != expected {
            return Err(bracel_data::conflict());
        }
    }
    jobs::dispatch(
        tx,
        &MailJob { delivery_id: id },
        &format!("mail:{id}"),
        &jobs::DispatchOptions {
            queue: "mail".into(),
            ..Default::default()
        },
    )
    .await?;
    Ok(id)
}
pub fn register_mail(
    worker: &mut jobs::Worker,
    db: DatabaseConnection,
    mailer: bracel_integrations::mail::Mailer,
) -> Result<(), &'static str> {
    worker.register_typed(move|job:MailJob|{let(db,mailer)=(db.clone(),mailer.clone());async move{
        let row=db.query_one_raw(sql("SELECT m.*,COALESCE(p.enabled,true) AS enabled FROM bracel_mail_delivery m LEFT JOIN bracel_notification_preferences p ON p.scope=m.scope AND p.channel='mail' WHERE m.id=$1",vec![job.delivery_id.into()])).await.map_err(|_|jobs::Failure::Retry("database"))?.ok_or(jobs::Failure::Permanent("missing_delivery"))?;
        let result=async{
            if row.try_get::<String>("","status")?=="accepted"{return Ok::<(),bracel::sea_orm::DbErr>(());}
            if !row.try_get::<bool>("","enabled")?{db.execute_raw(sql("UPDATE bracel_mail_delivery SET status='suppressed' WHERE id=$1",vec![job.delivery_id.into()])).await?;return Ok(());}
            Ok(())
        }.await;
        result.map_err(|_|jobs::Failure::Retry("database"))?;
        if !row.try_get::<bool>("","enabled").unwrap_or(false)||row.try_get::<String>("","status").unwrap_or_default()=="accepted"{return Ok(());}
        let field=|name|row.try_get::<String>("",name).map_err(|_|jobs::Failure::Permanent("invalid_delivery"));
        let message=bracel_integrations::mail::message(&field("sender")?,&field("recipient")?,&field("subject")?,&field("body")?).map_err(|_|jobs::Failure::Permanent("invalid_delivery"))?;
        if let Err(error) = mailer.send(message).await {
            if matches!(error, bracel_integrations::Error::Rejected | bracel_integrations::Error::InvalidInput) {
                db.execute_raw(sql("UPDATE bracel_mail_delivery SET status='failed' WHERE id=$1",vec![job.delivery_id.into()])).await.map_err(|_|jobs::Failure::Retry("database"))?;
                return Err(jobs::Failure::Permanent("mail_rejected"));
            }
            return Err(jobs::Failure::Retry("mail_unavailable"));
        }
        db.execute_raw(sql("UPDATE bracel_mail_delivery SET status='accepted',accepted_at=clock_timestamp() WHERE id=$1",vec![job.delivery_id.into()])).await.map_err(|_|jobs::Failure::Retry("database"))?;Ok(())
    }})
}

async fn receive_webhook(
    db: &DatabaseConnection,
    provider: &str,
    event_id: &str,
    payload: Value,
) -> Result<bool, AppError> {
    if provider.is_empty()
        || provider.len() > 100
        || event_id.is_empty()
        || event_id.len() > 200
        || payload.to_string().len() > 65536
    {
        return Err(bracel_data::conflict());
    }
    let tx = db.begin().await?;
    let inserted=tx.execute_raw(sql("INSERT INTO bracel_webhook_inbox(provider,event_id,payload) VALUES($1,$2,$3::jsonb) ON CONFLICT DO NOTHING",vec![provider.into(),event_id.into(),payload.to_string().into()])).await?.rows_affected()==1;
    let row=tx.query_one_raw(sql("SELECT payload::text AS payload FROM bracel_webhook_inbox WHERE provider=$1 AND event_id=$2",vec![provider.into(),event_id.into()])).await?.ok_or_else(bracel_data::conflict)?;
    if serde_json::from_str::<Value>(&row.try_get::<String>("", "payload")?)
        .map_err(|_| bracel_data::conflict())?
        != payload
    {
        return Err(bracel_data::conflict());
    }
    if inserted {
        jobs::dispatch(
            &tx,
            &IncomingJob {
                provider: provider.into(),
                event_id: event_id.into(),
            },
            &format!("incoming:{}", Uuid::now_v7()),
            &jobs::DispatchOptions {
                queue: "incoming".into(),
                ..Default::default()
            },
        )
        .await?;
    }
    tx.commit().await?;
    Ok(inserted)
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IncomingJob {
    pub provider: String,
    pub event_id: String,
}
impl jobs::TypedJob for IncomingJob {
    const KIND: &'static str = "delivery.incoming";
}

/// Complete this receipt in the same transaction as the application's database effects.
pub struct Incoming {
    pub transaction: DatabaseTransaction,
    pub payload: Value,
    provider: String,
    event_id: String,
}
impl Incoming {
    pub async fn begin(
        db: &DatabaseConnection,
        job: &IncomingJob,
    ) -> Result<Option<Self>, AppError> {
        let tx = db.begin().await?;
        tx.execute_unprepared("SET LOCAL statement_timeout='5s'")
            .await?;
        let row=tx.query_one_raw(sql("SELECT payload::text AS payload,processed_at IS NOT NULL AS processed FROM bracel_webhook_inbox WHERE provider=$1 AND event_id=$2 FOR UPDATE",vec![job.provider.clone().into(),job.event_id.clone().into()])).await?.ok_or_else(bracel_data::conflict)?;
        if row.try_get::<bool>("", "processed")? {
            tx.rollback().await?;
            return Ok(None);
        }
        Ok(Some(Self {
            transaction: tx,
            payload: serde_json::from_str(&row.try_get::<String>("", "payload")?)
                .map_err(|_| bracel_data::conflict())?,
            provider: job.provider.clone(),
            event_id: job.event_id.clone(),
        }))
    }
    pub async fn complete(self) -> Result<(), AppError> {
        self.transaction.execute_raw(sql("UPDATE bracel_webhook_inbox SET processed_at=clock_timestamp() WHERE provider=$1 AND event_id=$2",vec![self.provider.into(),self.event_id.into()])).await?;
        self.transaction.commit().await?;
        Ok(())
    }
}
