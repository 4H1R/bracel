use super::*;
use sea_orm::{DatabaseConnection, DatabaseTransaction, TransactionTrait};
use serde::de::DeserializeOwned;

pub const UPGRADE_QUEUES: &str = "ALTER TABLE bracel_jobs ADD COLUMN queue text NOT NULL DEFAULT 'default', ADD COLUMN priority integer NOT NULL DEFAULT 0, ADD COLUMN dispatch_delay_seconds bigint NOT NULL DEFAULT 0; CREATE INDEX bracel_jobs_queue_due ON bracel_jobs(queue,status,available_at); CREATE TABLE bracel_calendar(name text PRIMARY KEY,expression text NOT NULL,timezone text NOT NULL,kind text NOT NULL,version integer NOT NULL,payload jsonb NOT NULL,max_attempts integer NOT NULL,next_due_at timestamptz NOT NULL,last_job uuid,enabled boolean NOT NULL DEFAULT true);";
pub trait TypedJob: Serialize + DeserializeOwned + Send + 'static {
    const KIND: &'static str;
    const VERSION: i32 = 1;
}
#[derive(Clone)]
pub struct DispatchOptions {
    pub queue: String,
    pub delay: Duration,
    pub priority: i32,
    pub attempts: i32,
}
impl Default for DispatchOptions {
    fn default() -> Self {
        Self {
            queue: "default".into(),
            delay: Duration::ZERO,
            priority: 0,
            attempts: 5,
        }
    }
}
pub async fn dispatch<T: TypedJob>(
    tx: &DatabaseTransaction,
    payload: &T,
    dedupe: &str,
    options: &DispatchOptions,
) -> Result<Uuid, DbErr> {
    if !key(&options.queue)
        || options.delay.as_secs() > 31536000
        || !(-100..=100).contains(&options.priority)
    {
        return Err(invalid());
    }
    let spec = JobSpec {
        kind: T::KIND.into(),
        version: T::VERSION,
        payload: serde_json::to_value(payload).map_err(|_| invalid())?,
        dedupe_key: dedupe.into(),
        max_attempts: options.attempts,
    };
    spec.validate()?;
    if dedupe.starts_with("schedule:") {
        return Err(invalid());
    }
    let id = Uuid::now_v7();
    tx.execute_raw(statement("INSERT INTO bracel_jobs(id,kind,payload_version,payload,dedupe_key,max_attempts,queue,priority,dispatch_delay_seconds,available_at) VALUES($1,$2,$3,$4::jsonb,$5,$6,$7,$8,$9,clock_timestamp()+$9*interval '1 second') ON CONFLICT(dedupe_key) DO NOTHING",vec![id.into(),spec.kind.into(),spec.version.into(),spec.payload.to_string().into(),dedupe.into(),options.attempts.into(),options.queue.clone().into(),options.priority.into(),(options.delay.as_secs() as i64).into()])).await?;
    let row=tx.query_one_raw(statement("SELECT id,kind,payload_version,payload::text AS payload,max_attempts,queue,priority,dispatch_delay_seconds FROM bracel_jobs WHERE dedupe_key=$1",vec![dedupe.into()])).await?.ok_or_else(invalid)?;
    if row.try_get::<i64>("", "dispatch_delay_seconds")? != options.delay.as_secs() as i64
        || row.try_get::<String>("", "kind")? != T::KIND
        || row.try_get::<i32>("", "payload_version")? != T::VERSION
        || serde_json::from_str::<Value>(&row.try_get::<String>("", "payload")?)
            .map_err(|_| invalid())?
            != spec.payload
        || row.try_get::<String>("", "queue")? != options.queue
        || row.try_get::<i32>("", "priority")? != options.priority
        || row.try_get::<i32>("", "max_attempts")? != options.attempts
    {
        return Err(DbErr::Custom("Job deduplication conflict".into()));
    }
    row.try_get("", "id")
}

impl Worker {
    pub fn register_typed<T, F, Fut>(&mut self, handler: F) -> Result<(), &'static str>
    where
        T: TypedJob,
        F: Fn(T) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<(), Failure>> + Send + 'static,
    {
        self.register(T::KIND, T::VERSION, move |value| {
            let decoded = serde_json::from_value::<T>(value);
            let future = decoded.ok().map(&handler);
            async move {
                match future {
                    Some(future) => future.await,
                    None => Err(Failure::Permanent("invalid_payload")),
                }
            }
        })
    }
    pub async fn run_parallel(
        self: std::sync::Arc<Self>,
        db: DatabaseConnection,
        queue: String,
        concurrency: usize,
        timeout: Duration,
        stop: tokio::sync::watch::Receiver<bool>,
    ) -> Result<(), DbErr> {
        if !(1..=64).contains(&concurrency) || !key(&queue) {
            return Err(invalid());
        }
        let mut tasks = tokio::task::JoinSet::new();
        let failed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        for _ in 0..concurrency {
            let (worker, db, queue, mut stop) =
                (self.clone(), db.clone(), queue.clone(), stop.clone());
            let failed = failed.clone();
            tasks.spawn(async move{
                while !*stop.borrow() && !failed.load(std::sync::atomic::Ordering::Relaxed){
                    match worker.tick_queue(&db,&queue,timeout).await {
                        Ok(false)=>{tokio::select!{_=tokio::time::sleep(Duration::from_millis(500))=>{},_=stop.changed()=>break}},
                        Ok(true)=>{},
                        Err(error)=>{failed.store(true,std::sync::atomic::Ordering::Relaxed);return Err(error);}
                    }
                }
                Ok::<_,DbErr>(())
            });
        }
        let mut failure = None;
        while let Some(result) = tasks.join_next().await {
            if let Err(error) =
                result.unwrap_or_else(|_| Err(DbErr::Custom("Worker task stopped".into())))
            {
                failed.store(true, std::sync::atomic::Ordering::Relaxed);
                failure = Some(error);
            }
        }
        failure.map_or(Ok(()), Err)
    }
}

/// Calendar schedules use cron's timezone rules and coalesce missed occurrences.
pub async fn calendar(
    db: &DatabaseConnection,
    name: &str,
    expression: &str,
    timezone: &str,
    spec: &JobSpec,
) -> Result<(), DbErr> {
    spec.validate()?;
    if !key(name) || expression.len() > 100 {
        return Err(invalid());
    }
    let schedule = expression
        .parse::<cron::Schedule>()
        .map_err(|_| invalid())?;
    let zone = timezone.parse::<chrono_tz::Tz>().map_err(|_| invalid())?;
    let now = db
        .query_one_raw(statement("SELECT clock_timestamp() AS now", vec![]))
        .await?
        .ok_or_else(invalid)?
        .try_get::<chrono::DateTime<chrono::Utc>>("", "now")?;
    let next = schedule
        .after(&now.with_timezone(&zone))
        .next()
        .ok_or_else(invalid)?
        .with_timezone(&chrono::Utc);
    db.execute_raw(statement("INSERT INTO bracel_calendar(name,expression,timezone,kind,version,payload,max_attempts,next_due_at) VALUES($1,$2,$3,$4,$5,$6::jsonb,$7,$8) ON CONFLICT(name) DO UPDATE SET expression=excluded.expression,timezone=excluded.timezone,kind=excluded.kind,version=excluded.version,payload=excluded.payload,max_attempts=excluded.max_attempts",vec![name.into(),expression.into(),timezone.into(),spec.kind.clone().into(),spec.version.into(),spec.payload.to_string().into(),spec.max_attempts.into(),next.into()])).await?;
    Ok(())
}
pub async fn tick_calendar(db: &DatabaseConnection) -> Result<u64, DbErr> {
    let tx = db.begin().await?;
    let rows=tx.query_all_raw(statement("SELECT *,payload::text AS payload_text,clock_timestamp() AS now FROM bracel_calendar WHERE enabled AND next_due_at<=clock_timestamp() ORDER BY next_due_at,name FOR UPDATE SKIP LOCKED LIMIT 100",vec![])).await?;
    let count = rows.len() as u64;
    for row in rows {
        let name: String = row.try_get("", "name")?;
        let schedule = row
            .try_get::<String>("", "expression")?
            .parse::<cron::Schedule>()
            .map_err(|_| invalid())?;
        let zone = row
            .try_get::<String>("", "timezone")?
            .parse::<chrono_tz::Tz>()
            .map_err(|_| invalid())?;
        let now: chrono::DateTime<chrono::Utc> = row.try_get("", "now")?;
        let next = schedule
            .after(&now.with_timezone(&zone))
            .next()
            .map(|next| next.with_timezone(&chrono::Utc));
        let previous: Option<Uuid> = row.try_get("", "last_job")?;
        let busy = if let Some(id) = previous {
            tx.query_one_raw(statement(
                "SELECT id FROM bracel_jobs WHERE id=$1 AND status IN ('pending','running')",
                vec![id.into()],
            ))
            .await?
            .is_some()
        } else {
            false
        };
        let id = if busy {
            previous
        } else {
            let id = Uuid::now_v7();
            let due: chrono::DateTime<chrono::Utc> = row.try_get("", "next_due_at")?;
            tx.execute_raw(statement("INSERT INTO bracel_jobs(id,kind,payload_version,payload,dedupe_key,max_attempts) VALUES($1,$2,$3,$4::jsonb,$5,$6)",vec![id.into(),row.try_get::<String>("","kind")?.into(),row.try_get::<i32>("","version")?.into(),row.try_get::<String>("","payload_text")?.into(),format!("schedule:calendar:{name}:{due}").into(),row.try_get::<i32>("","max_attempts")?.into()])).await?;
            Some(id)
        };
        tx.execute_raw(statement(
            "UPDATE bracel_calendar SET next_due_at=COALESCE($2,next_due_at),enabled=($2 IS NOT NULL),last_job=$3 WHERE name=$1",
            vec![name.into(), next.into(), id.into()],
        ))
        .await?;
    }
    tx.commit().await?;
    Ok(count)
}
