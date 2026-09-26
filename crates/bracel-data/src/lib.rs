use bracel::{
    axum::http::StatusCode,
    http::error::AppError,
    sea_orm::{
        ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbBackend, Statement,
        TransactionTrait,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

/// Install only from an application's explicit migration. This SQL is immutable once released.
pub const SCHEMA: &str = "
CREATE TABLE bracel_idempotency (scope text NOT NULL, key text NOT NULL, fingerprint text NOT NULL, status integer NOT NULL, response jsonb NOT NULL, expires_at timestamptz NOT NULL, PRIMARY KEY(scope,key));
CREATE INDEX bracel_idempotency_expiry ON bracel_idempotency(expires_at);
CREATE TABLE bracel_memberships (tenant uuid NOT NULL, principal text NOT NULL, role text NOT NULL CHECK(role IN ('reader','writer','admin')), PRIMARY KEY(tenant,principal));
CREATE TABLE bracel_audit (id uuid PRIMARY KEY, scope text NOT NULL, actor text NOT NULL, action text NOT NULL, resource uuid NOT NULL, metadata jsonb NOT NULL, created_at timestamptz NOT NULL DEFAULT clock_timestamp());
CREATE INDEX bracel_audit_scope ON bracel_audit(scope,created_at,id);";

pub fn sql(query: &str, values: Vec<bracel::sea_orm::Value>) -> Statement {
    Statement::from_sql_and_values(DbBackend::Postgres, query, values)
}
pub fn conflict() -> AppError {
    AppError::new(
        StatusCode::CONFLICT,
        "The operation conflicts with the current state",
    )
}

#[derive(Clone, Serialize, Deserialize)]
pub struct StoredResponse {
    pub status: u16,
    pub body: Value,
}
pub enum Attempt {
    Replay(StoredResponse),
    New(Mutation),
}
pub struct Mutation {
    pub transaction: DatabaseTransaction,
    scope: String,
    key: String,
    fingerprint: String,
}

impl Mutation {
    #[tracing::instrument(skip_all, name = "database.idempotent_transaction")]
    pub async fn begin(
        db: &DatabaseConnection,
        scope: &str,
        operation: &str,
        key: &str,
        input: &Value,
    ) -> Result<Attempt, AppError> {
        if !(1..=200).contains(&key.len())
            || !key.bytes().all(|c| c.is_ascii_graphic())
            || scope.len() > 2048
            || operation.len() > 100
        {
            return Err(AppError::new(
                StatusCode::BAD_REQUEST,
                "Invalid idempotency context",
            ));
        }
        let scope = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&(scope, operation)).expect("context"))
        );
        let fingerprint = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(input).expect("JSON"))
        );
        let transaction = db.begin().await?;
        transaction
            .execute_unprepared("SET LOCAL statement_timeout='5s'")
            .await?;
        transaction
            .query_one_raw(sql(
                "SELECT pg_advisory_xact_lock(hashtextextended($1,0))",
                vec![format!("{scope}:{key}").into()],
            ))
            .await?;
        transaction.execute_raw(sql("DELETE FROM bracel_idempotency WHERE scope=$1 AND key=$2 AND expires_at<=clock_timestamp()",vec![scope.clone().into(),key.into()])).await?;
        if let Some(row) = transaction.query_one_raw(sql("SELECT fingerprint,status,response::text AS response FROM bracel_idempotency WHERE scope=$1 AND key=$2",vec![scope.clone().into(),key.into()])).await? {
            if row.try_get::<String>("", "fingerprint")? != fingerprint { return Err(conflict()); }
            let result = StoredResponse { status: row.try_get::<i32>("", "status")? as u16, body: serde_json::from_str(&row.try_get::<String>("", "response")?).map_err(|_| conflict())? };
            transaction.rollback().await?;
            return Ok(Attempt::Replay(result));
        }
        Ok(Attempt::New(Self {
            transaction,
            scope,
            key: key.into(),
            fingerprint,
        }))
    }
    #[tracing::instrument(skip_all, name = "database.commit")]
    pub async fn finish(self, status: u16, body: Value) -> Result<StoredResponse, AppError> {
        if !(200..300).contains(&status) || body.to_string().len() > 65536 {
            return Err(conflict());
        }
        self.transaction.execute_raw(sql("INSERT INTO bracel_idempotency(scope,key,fingerprint,status,response,expires_at) VALUES($1,$2,$3,$4,$5::jsonb,clock_timestamp()+interval '24 hours')",
            vec![self.scope.into(),self.key.into(),self.fingerprint.into(),i32::from(status).into(),body.to_string().into()])).await?;
        self.transaction.commit().await?;
        Ok(StoredResponse { status, body })
    }
}

pub fn expected_version(header: Option<&str>) -> Result<i64, AppError> {
    let value = header
        .ok_or_else(|| AppError::new(StatusCode::PRECONDITION_REQUIRED, "If-Match is required"))?;
    value
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .and_then(|v| v.parse::<i64>().ok())
        .filter(|v| *v > 0)
        .ok_or_else(|| {
            AppError::new(
                StatusCode::BAD_REQUEST,
                "If-Match must contain one strong version tag",
            )
        })
}
pub fn stale() -> AppError {
    AppError::new(
        StatusCode::PRECONDITION_FAILED,
        "Resource version has changed",
    )
}

pub async fn audit(
    tx: &DatabaseTransaction,
    scope: &str,
    actor: &str,
    action: &str,
    resource: uuid::Uuid,
    metadata: Value,
) -> Result<(), AppError> {
    if action.len() > 100
        || !action
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
        || metadata.to_string().len() > 4096
    {
        return Err(conflict());
    }
    tx.execute_raw(sql("INSERT INTO bracel_audit(id,scope,actor,action,resource,metadata) VALUES($1,$2,$3,$4,$5,$6::jsonb)", vec![uuid::Uuid::now_v7().into(),scope.into(),actor.into(),action.into(),resource.into(),metadata.to_string().into()])).await?;
    Ok(())
}

pub struct TenantScope {
    tenant: uuid::Uuid,
    role: String,
}
impl TenantScope {
    pub async fn resolve(
        db: &impl ConnectionTrait,
        tenant: uuid::Uuid,
        principal: &bracel::identity::Principal,
    ) -> Result<Self, AppError> {
        let row = db
            .query_one_raw(sql(
                "SELECT role FROM bracel_memberships WHERE tenant=$1 AND principal=$2 FOR SHARE",
                vec![tenant.into(), principal.cursor_scope().into()],
            ))
            .await?
            .ok_or_else(|| AppError::new(StatusCode::FORBIDDEN, "Tenant membership is required"))?;
        Ok(Self {
            tenant,
            role: row.try_get("", "role")?,
        })
    }
    pub fn id(&self) -> uuid::Uuid {
        self.tenant
    }
    pub fn key(&self) -> String {
        format!("tenant:{}", self.tenant)
    }
    pub fn require_write(&self) -> Result<(), AppError> {
        if matches!(self.role.as_str(), "writer" | "admin") {
            Ok(())
        } else {
            Err(AppError::new(
                StatusCode::FORBIDDEN,
                "Tenant write permission is required",
            ))
        }
    }
    pub fn require_admin(&self) -> Result<(), AppError> {
        if self.role == "admin" {
            Ok(())
        } else {
            Err(AppError::new(
                StatusCode::FORBIDDEN,
                "Tenant administrator permission is required",
            ))
        }
    }
    pub fn role(&self) -> &str {
        &self.role
    }
}

/// Bounded maintenance; replay guarantees end when an idempotency record expires.
pub async fn cleanup_idempotency(db: &impl ConnectionTrait) -> Result<u64, AppError> {
    Ok(db.execute_raw(sql("DELETE FROM bracel_idempotency WHERE (scope,key) IN (SELECT scope,key FROM bracel_idempotency WHERE expires_at<=clock_timestamp() LIMIT 500 FOR UPDATE SKIP LOCKED)",vec![])).await?.rows_affected())
}
