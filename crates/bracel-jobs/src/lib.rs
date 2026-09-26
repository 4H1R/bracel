//! PostgreSQL lease queue. Enqueue on the business transaction for atomic delivery intent.
mod options;
pub use options::*;
use sea_orm::{ConnectionTrait, DbBackend, DbErr, QueryResult, Statement};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, future::Future, pin::Pin, time::Duration};
use uuid::Uuid;

/// Initial migration SQL, frozen after release. Add separately named upgrade SQL for future schema changes.
pub const SCHEMA: &str = "
CREATE TABLE bracel_jobs (
 id uuid PRIMARY KEY, kind text NOT NULL, payload_version integer NOT NULL,
 payload jsonb NOT NULL, dedupe_key text NOT NULL UNIQUE,
 available_at timestamptz NOT NULL DEFAULT clock_timestamp(),
 attempts integer NOT NULL DEFAULT 0, max_attempts integer NOT NULL CHECK(max_attempts BETWEEN 1 AND 32),
 lease_token uuid, lease_until timestamptz, status text NOT NULL DEFAULT 'pending',
 last_error_code text, created_at timestamptz NOT NULL DEFAULT clock_timestamp(), finished_at timestamptz,
 CHECK(status IN ('pending','running','succeeded','failed')));
CREATE INDEX bracel_jobs_due ON bracel_jobs(status,available_at,lease_until);
CREATE TABLE bracel_job_replays (id uuid PRIMARY KEY, job_id uuid NOT NULL REFERENCES bracel_jobs(id), reason text NOT NULL, created_at timestamptz NOT NULL DEFAULT clock_timestamp());
CREATE TABLE bracel_schedules (
 name text PRIMARY KEY, kind text NOT NULL, payload_version integer NOT NULL, payload jsonb NOT NULL,
 every_seconds integer NOT NULL CHECK(every_seconds BETWEEN 1 AND 31536000),
 max_attempts integer NOT NULL CHECK(max_attempts BETWEEN 1 AND 32),
 next_due_at timestamptz NOT NULL, enabled boolean NOT NULL DEFAULT true);";

fn statement(sql: &str, values: Vec<sea_orm::Value>) -> Statement {
    Statement::from_sql_and_values(DbBackend::Postgres, sql, values)
}
fn invalid() -> DbErr {
    DbErr::Custom("Invalid job definition".into())
}
fn key(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 100
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_-.:".contains(&c))
}

#[derive(Serialize, Deserialize)]
pub struct JobSpec {
    pub kind: String,
    pub version: i32,
    pub payload: Value,
    pub dedupe_key: String,
    pub max_attempts: i32,
}
impl JobSpec {
    fn validate(&self) -> Result<(), DbErr> {
        if !key(&self.kind)
            || self.version < 1
            || !(1..=32).contains(&self.max_attempts)
            || self.payload.to_string().len() > 65536
            || self.dedupe_key.is_empty()
            || self.dedupe_key.len() > 200
        {
            return Err(invalid());
        }
        Ok(())
    }
}
/// Duplicate keys return the original ID only when the intent matches.
pub async fn enqueue(db: &impl ConnectionTrait, spec: &JobSpec) -> Result<Uuid, DbErr> {
    spec.validate()?;
    if spec.dedupe_key.starts_with("schedule:") {
        return Err(invalid());
    }
    db.execute_raw(statement("INSERT INTO bracel_jobs(id,kind,payload_version,payload,dedupe_key,max_attempts) VALUES($1,$2,$3,$4::jsonb,$5,$6) ON CONFLICT(dedupe_key) DO NOTHING",
        vec![Uuid::now_v7().into(),spec.kind.clone().into(),spec.version.into(),spec.payload.to_string().into(),spec.dedupe_key.clone().into(),spec.max_attempts.into()])).await?;
    let row=db.query_one_raw(statement("SELECT id,kind,payload_version,payload::text AS payload,max_attempts FROM bracel_jobs WHERE dedupe_key=$1",vec![spec.dedupe_key.clone().into()])).await?.ok_or_else(invalid)?;
    let payload: String = row.try_get("", "payload")?;
    if row.try_get::<String>("", "kind")? != spec.kind
        || row.try_get::<i32>("", "payload_version")? != spec.version
        || serde_json::from_str::<Value>(&payload).map_err(|_| invalid())? != spec.payload
        || row.try_get::<i32>("", "max_attempts")? != spec.max_attempts
    {
        return Err(DbErr::Custom("Job deduplication conflict".into()));
    }
    row.try_get("", "id")
}
pub struct Job {
    pub id: Uuid,
    pub kind: String,
    pub version: i32,
    pub payload: Value,
    pub attempts: i32,
    pub max_attempts: i32,
    lease_token: Uuid,
}
pub async fn claim(db: &impl ConnectionTrait, lease: Duration) -> Result<Option<Job>, DbErr> {
    claim_queue(db, "default", lease).await
}
pub async fn claim_queue(
    db: &impl ConnectionTrait,
    queue: &str,
    lease: Duration,
) -> Result<Option<Job>, DbErr> {
    if !key(queue) {
        return Err(invalid());
    }
    let seconds = i32::try_from(lease.as_secs()).map_err(|_| invalid())?;
    if !(2..=3600).contains(&seconds) {
        return Err(invalid());
    }
    db.execute_raw(statement("UPDATE bracel_jobs SET status='failed',finished_at=clock_timestamp(),last_error_code='lease_exhausted' WHERE status='running' AND lease_until<=clock_timestamp() AND attempts>=max_attempts",vec![])).await?;
    let row=db.query_one_raw(statement("UPDATE bracel_jobs j SET status='running',attempts=j.attempts+1,lease_token=$1,lease_until=clock_timestamp()+$2*interval '1 second'
        FROM (SELECT id FROM bracel_jobs WHERE COALESCE(to_jsonb(bracel_jobs)->>'queue','default')=$3 AND attempts<max_attempts AND ((status='pending' AND available_at<=clock_timestamp()) OR (status='running' AND lease_until<=clock_timestamp()))
        ORDER BY COALESCE((to_jsonb(bracel_jobs)->>'priority')::integer,0) DESC,available_at,id FOR UPDATE SKIP LOCKED LIMIT 1) candidate WHERE j.id=candidate.id
        RETURNING j.id,j.kind,j.payload_version,j.payload::text AS payload,j.attempts,j.max_attempts,j.lease_token",vec![Uuid::now_v7().into(),seconds.into(),queue.into()])).await?;
    row.map(|r| {
        Ok(Job {
            id: r.try_get("", "id")?,
            kind: r.try_get("", "kind")?,
            version: r.try_get("", "payload_version")?,
            payload: serde_json::from_str(&r.try_get::<String>("", "payload")?)
                .map_err(|_| invalid())?,
            attempts: r.try_get("", "attempts")?,
            max_attempts: r.try_get("", "max_attempts")?,
            lease_token: r.try_get("", "lease_token")?,
        })
    })
    .transpose()
}
pub async fn complete(db: &impl ConnectionTrait, job: &Job) -> Result<bool, DbErr> {
    Ok(db.execute_raw(statement("UPDATE bracel_jobs SET status='succeeded',finished_at=clock_timestamp(),lease_token=NULL,lease_until=NULL,last_error_code=NULL WHERE id=$1 AND lease_token=$2 AND status='running' AND lease_until>clock_timestamp()",
        vec![job.id.into(),job.lease_token.into()])).await?.rows_affected()==1)
}
#[derive(Clone, Copy)]
pub enum Failure {
    Retry(&'static str),
    Permanent(&'static str),
}
pub async fn fail(db: &impl ConnectionTrait, job: &Job, failure: Failure) -> Result<bool, DbErr> {
    let (retry, code) = match failure {
        Failure::Retry(code) => (job.attempts < job.max_attempts, code),
        Failure::Permanent(code) => (false, code),
    };
    if !key(code) {
        return Err(invalid());
    }
    let delay = (5_i32.saturating_mul(2_i32.saturating_pow(job.attempts.saturating_sub(1) as u32)))
        .min(900)
        + (Uuid::now_v7().as_u128() % 5) as i32;
    Ok(db.execute_raw(statement("UPDATE bracel_jobs SET status=$3,last_error_code=$4,available_at=clock_timestamp()+$5*interval '1 second',finished_at=CASE WHEN $3='failed' THEN clock_timestamp() ELSE NULL END,lease_token=NULL,lease_until=NULL WHERE id=$1 AND lease_token=$2 AND status='running' AND lease_until>clock_timestamp()",
        vec![job.id.into(),job.lease_token.into(),(if retry {"pending"} else {"failed"}).into(),code.into(),delay.into()])).await?.rows_affected()==1)
}
#[derive(Serialize)]
pub struct JobStatus {
    pub id: Uuid,
    pub kind: String,
    pub status: String,
    pub attempts: i32,
    pub error_code: Option<String>,
}
fn status(row: QueryResult) -> Result<JobStatus, DbErr> {
    Ok(JobStatus {
        id: row.try_get("", "id")?,
        kind: row.try_get("", "kind")?,
        status: row.try_get("", "status")?,
        attempts: row.try_get("", "attempts")?,
        error_code: row.try_get("", "last_error_code")?,
    })
}
pub async fn failed(db: &impl ConnectionTrait) -> Result<Vec<JobStatus>, DbErr> {
    db.query_all_raw(statement("SELECT id,kind,status,attempts,last_error_code FROM bracel_jobs WHERE status='failed' ORDER BY finished_at,id LIMIT 100",vec![])).await?.into_iter().map(status).collect()
}
pub async fn replay(db: &impl ConnectionTrait, id: Uuid, reason: &str) -> Result<bool, DbErr> {
    if reason.trim().is_empty() || reason.len() > 200 {
        return Err(invalid());
    }
    Ok(db.execute_raw(statement("WITH replay AS (UPDATE bracel_jobs SET status='pending',attempts=0,available_at=clock_timestamp(),finished_at=NULL,last_error_code=NULL WHERE id=$1 AND status='failed' RETURNING id)
        INSERT INTO bracel_job_replays(id,job_id,reason) SELECT $2,id,$3 FROM replay",vec![id.into(),Uuid::now_v7().into(),reason.into()])).await?.rows_affected()==1)
}

type Handler =
    Box<dyn Fn(Value) -> Pin<Box<dyn Future<Output = Result<(), Failure>> + Send>> + Send + Sync>;
#[derive(Default)]
pub struct Worker {
    handlers: BTreeMap<(String, i32), Handler>,
}
impl Worker {
    pub fn register<F, Fut>(
        &mut self,
        kind: &str,
        version: i32,
        handler: F,
    ) -> Result<(), &'static str>
    where
        F: Fn(Value) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<(), Failure>> + Send + 'static,
    {
        if !key(kind) || version < 1 || self.handlers.contains_key(&(kind.into(), version)) {
            return Err("Invalid or duplicate job handler");
        }
        self.handlers.insert(
            (kind.into(), version),
            Box::new(move |p| Box::pin(handler(p))),
        );
        Ok(())
    }
    pub async fn tick(&self, db: &impl ConnectionTrait, timeout: Duration) -> Result<bool, DbErr> {
        self.tick_queue(db, "default", timeout).await
    }
    #[tracing::instrument(skip_all,fields(queue=queue))]
    pub async fn tick_queue(
        &self,
        db: &impl ConnectionTrait,
        queue: &str,
        timeout: Duration,
    ) -> Result<bool, DbErr> {
        if timeout.is_zero() || timeout > Duration::from_secs(3590) {
            return Err(invalid());
        }
        let Some(job) = claim_queue(db, queue, timeout + Duration::from_secs(5)).await? else {
            return Ok(false);
        };
        let result = if let Some(handler) = self.handlers.get(&(job.kind.clone(), job.version)) {
            tokio::time::timeout(timeout, handler(job.payload.clone()))
                .await
                .unwrap_or(Err(Failure::Retry("timeout")))
        } else {
            Err(Failure::Permanent("unknown_job"))
        };
        match result {
            Ok(()) => {
                complete(db, &job).await?;
            }
            Err(error) => {
                fail(db, &job, error).await?;
            }
        }
        Ok(true)
    }
    /// Stop claiming immediately on shutdown; finish the current bounded attempt first.
    pub async fn run(
        &self,
        db: &impl ConnectionTrait,
        mut stop: tokio::sync::watch::Receiver<bool>,
    ) -> Result<(), DbErr> {
        while !*stop.borrow() {
            if !self.tick(db, Duration::from_secs(25)).await? {
                tokio::select! { _=tokio::time::sleep(Duration::from_millis(500))=>{}, changed=stop.changed()=>{if changed.is_err(){break;}} }
            }
        }
        Ok(())
    }
}

/// Persist an interval schedule explicitly. Updating its definition preserves the next due time.
pub async fn schedule(
    db: &impl ConnectionTrait,
    name: &str,
    every_seconds: i32,
    spec: &JobSpec,
) -> Result<(), DbErr> {
    spec.validate()?;
    if !key(name) || !(1..=31536000).contains(&every_seconds) {
        return Err(invalid());
    }
    db.execute_raw(statement("INSERT INTO bracel_schedules(name,kind,payload_version,payload,every_seconds,next_due_at,max_attempts) VALUES($1,$2,$3,$4::jsonb,$5,clock_timestamp()+$5*interval '1 second',$6)
        ON CONFLICT(name) DO UPDATE SET kind=excluded.kind,payload_version=excluded.payload_version,payload=excluded.payload,every_seconds=excluded.every_seconds,max_attempts=excluded.max_attempts",
        vec![name.into(),spec.kind.clone().into(),spec.version.into(),spec.payload.to_string().into(),every_seconds.into(),spec.max_attempts.into()])).await?;
    Ok(())
}
/// Coalesce missed occurrences; row locking and insertion/advancement share one statement.
pub async fn tick_schedules(db: &impl ConnectionTrait) -> Result<u64, DbErr> {
    Ok(db.execute_raw(statement("WITH due AS (SELECT *,clock_timestamp() AS now FROM bracel_schedules WHERE enabled AND next_due_at<=clock_timestamp() ORDER BY next_due_at,name FOR UPDATE SKIP LOCKED LIMIT 100),
        inserted AS (INSERT INTO bracel_jobs(id,kind,payload_version,payload,dedupe_key,max_attempts)
        SELECT gen_random_uuid(),kind,payload_version,payload,'schedule:'||name||':'||extract(epoch FROM next_due_at)::text,max_attempts FROM due ON CONFLICT(dedupe_key) DO NOTHING)
        UPDATE bracel_schedules s SET next_due_at=d.next_due_at+(floor(extract(epoch FROM(d.now-d.next_due_at))/d.every_seconds)+1)*d.every_seconds*interval '1 second' FROM due d WHERE s.name=d.name",vec![])).await?.rows_affected())
}
pub async fn enable_schedule(
    db: &impl ConnectionTrait,
    name: &str,
    enabled: bool,
) -> Result<bool, DbErr> {
    Ok(db
        .execute_raw(statement(
            "UPDATE bracel_schedules SET enabled=$2 WHERE name=$1",
            vec![name.into(), enabled.into()],
        ))
        .await?
        .rows_affected()
        == 1)
}

pub async fn run_schedules(
    db: &impl ConnectionTrait,
    mut stop: tokio::sync::watch::Receiver<bool>,
) -> Result<(), DbErr> {
    while !*stop.borrow() {
        tick_schedules(db).await?;
        tokio::select! { _=tokio::time::sleep(Duration::from_secs(5))=>{}, changed=stop.changed()=>{if changed.is_err(){break;}} }
    }
    Ok(())
}
