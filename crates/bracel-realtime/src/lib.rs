use bracel::{
    axum::{
        http::StatusCode,
        response::sse::{Event, KeepAlive, Sse},
    },
    http::error::AppError,
    identity::BearerAuth,
    sea_orm::{
        ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbBackend, Statement,
        TransactionTrait,
    },
};
use futures_util::{Stream, stream};
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[cfg(feature = "websocket")]
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    collections::VecDeque,
    convert::Infallible,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore, watch};
use uuid::Uuid;

pub const SCHEMA: &str = "CREATE TABLE bracel_event_clock(singleton boolean PRIMARY KEY DEFAULT true CHECK(singleton),position bigint NOT NULL,floor bigint NOT NULL); INSERT INTO bracel_event_clock VALUES(true,0,0); CREATE TABLE bracel_events(position bigint PRIMARY KEY,id uuid NOT NULL UNIQUE,scope text NOT NULL,topic text NOT NULL,kind text NOT NULL,version integer NOT NULL,data jsonb NOT NULL,created_at timestamptz NOT NULL DEFAULT clock_timestamp()); CREATE INDEX bracel_events_subscription ON bracel_events(scope,topic,position); CREATE TABLE bracel_event_consumers(consumer text NOT NULL,event_id uuid NOT NULL,PRIMARY KEY(consumer,event_id));";
fn sql(query: &str, values: Vec<bracel::sea_orm::Value>) -> Statement {
    Statement::from_sql_and_values(DbBackend::Postgres, query, values)
}
fn invalid() -> AppError {
    AppError::new(
        StatusCode::UNPROCESSABLE_ENTITY,
        "Invalid event subscription or cursor",
    )
}
fn label(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 100
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
}

pub trait DomainEvent: Serialize {
    const KIND: &'static str;
    const VERSION: i32 = 1;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Envelope {
    pub id: Uuid,
    pub kind: String,
    pub version: i32,
    pub topic: String,
    pub data: Value,
}

pub async fn publish<E: DomainEvent>(
    tx: &DatabaseTransaction,
    scope: &str,
    topic: &str,
    event: &E,
) -> Result<Uuid, AppError> {
    publish_value(
        tx,
        scope,
        topic,
        E::KIND,
        E::VERSION,
        serde_json::to_value(event).map_err(|_| invalid())?,
    )
    .await
}
pub async fn publish_value(
    tx: &DatabaseTransaction,
    scope: &str,
    topic: &str,
    kind: &str,
    version: i32,
    data: Value,
) -> Result<Uuid, AppError> {
    if scope.is_empty()
        || scope.len() > 2048
        || !label(topic)
        || !label(kind)
        || version < 1
        || data.to_string().len() > 65536
    {
        return Err(invalid());
    }
    // Holding this transactional counter lock through commit makes positions safe resume markers.
    let row = tx
        .query_one_raw(sql(
            "UPDATE bracel_event_clock SET position=position+1 WHERE singleton RETURNING position",
            vec![],
        ))
        .await?
        .ok_or_else(invalid)?;
    let position: i64 = row.try_get("", "position")?;
    let id = Uuid::now_v7();
    tx.execute_raw(sql("INSERT INTO bracel_events(position,id,scope,topic,kind,version,data) VALUES($1,$2,$3,$4,$5,$6,$7::jsonb)",vec![position.into(),id.into(),scope.into(),topic.into(),kind.into(),version.into(),data.to_string().into()])).await?;
    Ok(id)
}
/// Commit this claim together with a consumer's database effects; false means already consumed.
pub async fn consume_once(
    tx: &DatabaseTransaction,
    consumer: &str,
    id: Uuid,
) -> Result<bool, AppError> {
    if !label(consumer) {
        return Err(invalid());
    }
    Ok(tx.execute_raw(sql("INSERT INTO bracel_event_consumers(consumer,event_id) VALUES($1,$2) ON CONFLICT DO NOTHING",vec![consumer.into(),id.into()])).await?.rows_affected()==1)
}

#[derive(Clone)]
pub struct EventStore {
    db: DatabaseConnection,
    connections: Arc<Semaphore>,
    stop: watch::Sender<bool>,
}
impl EventStore {
    pub fn new(db: DatabaseConnection, max_connections: usize) -> Result<Self, AppError> {
        if !(1..=10000).contains(&max_connections) {
            return Err(invalid());
        }
        let (stop, _) = watch::channel(false);
        Ok(Self {
            db,
            connections: Arc::new(Semaphore::new(max_connections)),
            stop,
        })
    }
    pub fn shutdown(&self) {
        let _ = self.stop.send_replace(true);
    }
    pub async fn subscribe(
        &self,
        scope: String,
        topic: String,
        cursor: Option<&str>,
        auth: BearerAuth,
        token: String,
    ) -> Result<Subscription, AppError> {
        if !label(&topic) || scope.is_empty() || scope.len() > 2048 {
            return Err(invalid());
        }
        let principal = auth
            .verify_access(&token)
            .await
            .map_err(|_| {
                AppError::new(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "Authentication unavailable",
                )
            })?
            .ok_or_else(|| AppError::new(StatusCode::UNAUTHORIZED, "Access token expired"))?;
        if principal.cursor_scope() != scope || !principal.allows("events:read") {
            return Err(AppError::new(
                StatusCode::FORBIDDEN,
                "Event permission is required",
            ));
        }
        let binding = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&(&scope, &topic)).expect("subscription"))
        );
        let head = self
            .db
            .query_one_raw(sql(
                "SELECT position,floor FROM bracel_event_clock WHERE singleton",
                vec![],
            ))
            .await?
            .ok_or_else(invalid)?;
        let floor: i64 = head.try_get("", "floor")?;
        let position = match cursor {
            Some(value) => value
                .strip_prefix(&format!("v1.{binding}."))
                .and_then(|v| v.parse::<i64>().ok())
                .ok_or_else(invalid)?,
            None => floor,
        };
        if position < floor {
            return Err(AppError::new(
                StatusCode::CONFLICT,
                "Event history expired; resynchronize from a snapshot",
            ));
        }
        if position < 0 || position > head.try_get::<i64>("", "position")? {
            return Err(invalid());
        }
        let permit = self.connections.clone().try_acquire_owned().map_err(|_| {
            AppError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "Stream connection capacity exceeded",
            )
        })?;
        Ok(Subscription {
            store: self.clone(),
            scope,
            topic,
            binding,
            position,
            pending: VecDeque::new(),
            auth,
            token,
            permit,
            stop: self.stop.subscribe(),
            started: Instant::now(),
        })
    }
    /// Explicit bounded prefix retention. Never invoke this automatically during construction.
    pub async fn prune(&self, through: i64) -> Result<u64, AppError> {
        let tx = self.db.begin().await?;
        let row = tx
            .query_one_raw(sql(
                "SELECT position,floor FROM bracel_event_clock WHERE singleton FOR UPDATE",
                vec![],
            ))
            .await?
            .ok_or_else(invalid)?;
        if through < row.try_get::<i64>("", "floor")?
            || through > row.try_get::<i64>("", "position")?
        {
            return Err(invalid());
        }
        let through = through.min(row.try_get::<i64>("", "floor")?.saturating_add(1000));
        let deleted = tx
            .execute_raw(sql(
                "DELETE FROM bracel_events WHERE position<=$1",
                vec![through.into()],
            ))
            .await?
            .rows_affected();
        tx.execute_raw(sql(
            "UPDATE bracel_event_clock SET floor=$1 WHERE singleton",
            vec![through.into()],
        ))
        .await?;
        tx.commit().await?;
        Ok(deleted)
    }
}

/// Transient JSON chunks (for example model output) are never written to the event log.
/// Dropping the response drops the source. The caller must authorize and bound its upstream work.
pub fn transient<S>(
    source: S,
    lifetime: Duration,
) -> Sse<impl Stream<Item = Result<Event, Infallible>> + Send>
where
    S: Stream<Item = Result<Value, AppError>> + Send + 'static,
{
    use futures_util::StreamExt;
    let deadline = tokio::time::Instant::now() + lifetime.min(Duration::from_secs(3600));
    let output = stream::unfold(
        (Box::pin(source), false),
        move |(mut source, ended)| async move {
            if ended {
                return None;
            }
            let (event, ended) = match tokio::time::timeout_at(deadline, source.next()).await {
                Ok(Some(Ok(value))) if value.to_string().len() <= 65536 => (
                    Event::default()
                        .event("chunk")
                        .json_data(value)
                        .expect("JSON value"),
                    false,
                ),
                Ok(None) => (Event::default().event("done").data("{}"), true),
                _ => (
                    Event::default()
                        .event("error")
                        .data("{\"code\":\"stream_stopped\"}"),
                    true,
                ),
            };
            Some((Ok(event), (source, ended)))
        },
    );
    Sse::new(output).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
}

pub struct Subscription {
    store: EventStore,
    scope: String,
    topic: String,
    binding: String,
    position: i64,
    pending: VecDeque<(i64, Envelope)>,
    auth: BearerAuth,
    token: String,
    permit: OwnedSemaphorePermit,
    stop: watch::Receiver<bool>,
    started: Instant,
}
impl Subscription {
    async fn next(&mut self) -> Result<Option<(String, Envelope)>, AppError> {
        loop {
            if *self.stop.borrow() || self.started.elapsed() > Duration::from_secs(3600) {
                return Ok(None);
            }
            let principal = self
                .auth
                .verify_access(&self.token)
                .await
                .map_err(|_| {
                    AppError::new(
                        StatusCode::SERVICE_UNAVAILABLE,
                        "Authentication unavailable",
                    )
                })?
                .ok_or_else(|| {
                    AppError::new(StatusCode::UNAUTHORIZED, "Access expired or revoked")
                })?;
            if !principal.allows("events:read") {
                return Err(AppError::new(
                    StatusCode::FORBIDDEN,
                    "Event permission revoked",
                ));
            }
            if let Some((position, event)) = self.pending.pop_front() {
                self.position = position;
                return Ok(Some((format!("v1.{}.{}", self.binding, position), event)));
            }
            let tx = self.store.db.begin().await?;
            tx.execute_unprepared("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
                .await?;
            tx.execute_unprepared("SET LOCAL statement_timeout='3s'")
                .await?;
            let head = tx
                .query_one_raw(sql(
                    "SELECT floor FROM bracel_event_clock WHERE singleton",
                    vec![],
                ))
                .await?
                .ok_or_else(invalid)?;
            if self.position < head.try_get::<i64>("", "floor")? {
                return Err(AppError::new(StatusCode::CONFLICT, "Event history expired"));
            }
            let rows=tx.query_all_raw(sql("SELECT position,id,kind,version,topic,data::text AS data FROM bracel_events WHERE scope=$1 AND topic=$2 AND position>$3 ORDER BY position LIMIT 64",vec![self.scope.clone().into(),self.topic.clone().into(),self.position.into()])).await?;
            tx.commit().await?;
            for row in rows {
                self.pending.push_back((
                    row.try_get("", "position")?,
                    Envelope {
                        id: row.try_get("", "id")?,
                        kind: row.try_get("", "kind")?,
                        version: row.try_get("", "version")?,
                        topic: row.try_get("", "topic")?,
                        data: serde_json::from_str(&row.try_get::<String>("", "data")?)
                            .map_err(|_| invalid())?,
                    },
                ));
            }
            if self.pending.is_empty() {
                tokio::select! {_=tokio::time::sleep(Duration::from_millis(250))=>{},_=self.stop.changed()=>return Ok(None)}
            }
        }
    }
    pub fn sse(mut self) -> Sse<impl Stream<Item = Result<Event, Infallible>> + Send + 'static> {
        let (sender, receiver) = tokio::sync::mpsc::channel(16);
        let mut stopping = self.stop.clone();
        tokio::spawn(async move {
            let producer = async {
                loop {
                    let _ = &self.permit;
                    let (event, end) = match self.next().await {
                        Ok(Some((id, event))) => (
                            Event::default()
                                .id(id)
                                .event("message")
                                .json_data(event)
                                .expect("event"),
                            false,
                        ),
                        Ok(None) => break,
                        Err(_) => (Event::default().event("resync_required").data("{}"), true),
                    };
                    if !matches!(
                        tokio::time::timeout(Duration::from_secs(5), sender.send(Ok(event))).await,
                        Ok(Ok(()))
                    ) || end
                    {
                        break;
                    }
                }
            };
            tokio::select! {
                _=tokio::time::timeout(Duration::from_secs(3600),producer)=>{},
                _=sender.closed()=>{},
                _=stopping.changed()=>{},
            }
        });
        Sse::new(stream::unfold(receiver, |mut receiver| async move {
            receiver.recv().await.map(|event| (event, receiver))
        }))
        .keep_alive(
            KeepAlive::new()
                .interval(Duration::from_secs(15))
                .text("heartbeat"),
        )
    }
    #[cfg(feature = "websocket")]
    pub async fn websocket(self, socket: bracel::axum::extract::ws::WebSocket) {
        self.websocket_with(socket, |_: bracel::identity::Principal, _: Value| async {
            Err(AppError::new(
                StatusCode::BAD_REQUEST,
                "Unsupported message",
            ))
        })
        .await;
    }
    /// Reverify the caller before every bounded application message; the handler owns its policy.
    #[cfg(feature = "websocket")]
    pub async fn websocket_with<F, Fut>(
        mut self,
        mut socket: bracel::axum::extract::ws::WebSocket,
        handler: F,
    ) where
        F: Fn(bracel::identity::Principal, Value) -> Fut + Send,
        Fut: std::future::Future<Output = Result<Value, AppError>> + Send,
    {
        use bracel::axum::extract::ws::{Message, WebSocket};
        async fn send(socket: &mut WebSocket, message: Message) -> bool {
            matches!(
                tokio::time::timeout(Duration::from_secs(5), socket.send(message)).await,
                Ok(Ok(()))
            )
        }
        let mut stopping = self.stop.clone();
        let session = async {
            let mut heartbeat = tokio::time::interval(Duration::from_secs(15));
            let mut last_message = Instant::now();
            loop {
                tokio::select! {
                    event=self.next()=>{
                        let Ok(Some((cursor,event)))=event else {let _=send(&mut socket,Message::Close(None)).await;break};
                        if !send(&mut socket,Message::Text(json!({"cursor":cursor,"event":event}).to_string().into())).await {break;}
                    },
                    message=socket.recv()=>{
                        last_message=Instant::now();
                        match message {
                            Some(Ok(Message::Ping(bytes)))=>{if !send(&mut socket,Message::Pong(bytes)).await {break;}},
                            Some(Ok(Message::Text(text)))=>{
                                if text.len()>8192 {break;}
                                let Ok(value)=serde_json::from_str::<bracel::http::extract::UniqueJson>(&text) else {break};
                                let Ok(Some(principal))=self.auth.verify_access(&self.token).await else {break};
                                if principal.cursor_scope()!=self.scope || !principal.allows("events:read") {break;}
                                let response=if value.0==json!({"type":"ping"}) {json!({"type":"pong"})} else {
                                    match tokio::time::timeout(Duration::from_secs(5),handler(principal,value.0)).await {
                                        Ok(Ok(reply)) if reply.to_string().len()<=65536=>reply,
                                        _=>json!({"type":"error","code":"message_rejected"}),
                                    }
                                };
                                if !send(&mut socket,Message::Text(response.to_string().into())).await {break;}
                            },
                            Some(Ok(Message::Pong(_)))=>{},
                            _=>break,
                        }
                    },
                    _=heartbeat.tick()=>{
                        if last_message.elapsed()>Duration::from_secs(45)||!send(&mut socket,Message::Ping(Vec::new().into())).await {break;}
                    },
                }
            }
        };
        tokio::select! {_=tokio::time::timeout(Duration::from_secs(3600),session)=>{},_=stopping.changed()=>{}}
    }
}

/// Read in the same repeatable-read transaction as a bounded resource snapshot.
pub async fn snapshot_cursor(
    tx: &DatabaseTransaction,
    scope: &str,
    topic: &str,
) -> Result<String, AppError> {
    let head = tx
        .query_one_raw(sql(
            "SELECT position FROM bracel_event_clock WHERE singleton",
            vec![],
        ))
        .await?
        .ok_or_else(invalid)?;
    let position: i64 = head.try_get("", "position")?;
    let binding = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&(scope, topic)).expect("subscription"))
    );
    Ok(format!("v1.{binding}.{position}"))
}
