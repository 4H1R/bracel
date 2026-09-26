use bracel::{
    axum::http::StatusCode,
    http::error::{AppError, IssueCode, ValidationErrors},
    sea_orm::{ConnectionTrait, DatabaseConnection, TransactionTrait},
};
use bracel_integrations::storage::Storage;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub const SCHEMA: &str = "CREATE TABLE bracel_files(id uuid PRIMARY KEY,scope text NOT NULL,object_id uuid NOT NULL UNIQUE,name text NOT NULL,size bigint NOT NULL,sha256 text NOT NULL,content_type text NOT NULL,status text NOT NULL DEFAULT 'pending',expires_at timestamptz NOT NULL DEFAULT clock_timestamp()+interval '1 hour',created_at timestamptz NOT NULL DEFAULT clock_timestamp(),CHECK(status IN ('pending','uploaded','available','deleted'))); CREATE INDEX bracel_files_cleanup ON bracel_files(status,expires_at); CREATE TABLE bracel_attachments(scope text NOT NULL,resource uuid NOT NULL,file_id uuid NOT NULL REFERENCES bracel_files(id),PRIMARY KEY(scope,resource,file_id));";
#[derive(Clone)]
pub struct Files {
    db: DatabaseConnection,
    storage: Storage,
    max_bytes: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Upload {
    pub name: String,
    pub size: u64,
    pub sha256: String,
    pub content_type: String,
}
#[derive(Serialize)]
pub struct File {
    pub id: Uuid,
    pub name: String,
    pub size: i64,
    pub content_type: String,
    pub status: String,
}
fn missing() -> AppError {
    AppError::new(StatusCode::NOT_FOUND, "File not found")
}
fn unavailable() -> AppError {
    AppError::new(StatusCode::SERVICE_UNAVAILABLE, "Storage unavailable")
}
fn invalid() -> AppError {
    let mut errors = ValidationErrors::default();
    errors.add(
        [],
        IssueCode::Custom,
        "File metadata or content does not match the upload policy.",
    );
    errors.into()
}
fn metadata(row: bracel::sea_orm::QueryResult) -> Result<File, AppError> {
    Ok(File {
        id: row.try_get("", "id")?,
        name: row.try_get("", "name")?,
        size: row.try_get("", "size")?,
        content_type: row.try_get("", "content_type")?,
        status: row.try_get("", "status")?,
    })
}
impl Files {
    pub fn new(db: DatabaseConnection, storage: Storage, max_bytes: u64) -> Result<Self, AppError> {
        if max_bytes == 0 || max_bytes > 16 * 1024 * 1024 {
            return Err(invalid());
        }
        Ok(Self {
            db,
            storage,
            max_bytes,
        })
    }
    pub async fn initialize(&self, scope: &str, input: Upload) -> Result<File, AppError> {
        if scope.is_empty()
            || scope.len() > 2048
            || input.name.is_empty()
            || input.name.len() > 200
            || input
                .name
                .chars()
                .any(|c| c.is_control() || matches!(c, '/' | '\\'))
            || input.size > self.max_bytes
            || input.sha256.len() != 64
            || !input.sha256.bytes().all(|c| c.is_ascii_hexdigit())
            || !matches!(
                input.content_type.as_str(),
                "text/plain"
                    | "application/octet-stream"
                    | "application/pdf"
                    | "image/png"
                    | "image/jpeg"
            )
        {
            return Err(invalid());
        }
        let id = Uuid::now_v7();
        self.db.execute_raw(sql("INSERT INTO bracel_files(id,scope,name,size,sha256,content_type,object_id) VALUES($1,$2,$3,$4,$5,$6,$7)",vec![id.into(),scope.into(),input.name.into(),(input.size as i64).into(),input.sha256.to_ascii_lowercase().into(),input.content_type.into(),Uuid::now_v7().into()])).await?;
        self.metadata(scope, id).await
    }
    pub async fn metadata(&self, scope: &str, id: Uuid) -> Result<File, AppError> {
        metadata(
            self.db
                .query_one_raw(sql(
                    "SELECT * FROM bracel_files WHERE id=$1 AND scope=$2 AND status<>'deleted'",
                    vec![id.into(), scope.into()],
                ))
                .await?
                .ok_or_else(missing)?,
        )
    }
    pub async fn upload(&self, scope: &str, id: Uuid, bytes: Vec<u8>) -> Result<File, AppError> {
        let tx = self.db.begin().await?;
        let row=tx.query_one_raw(sql("SELECT * FROM bracel_files WHERE id=$1 AND scope=$2 AND status IN ('pending','uploaded') AND expires_at>clock_timestamp() FOR UPDATE",vec![id.into(),scope.into()])).await?.ok_or_else(missing)?;
        validate_content(&row, &bytes, self.max_bytes)?;
        self.storage
            .put_at(scope, id, bytes)
            .await
            .map_err(|_| unavailable())?;
        tx.execute_raw(sql(
            "UPDATE bracel_files SET status='uploaded' WHERE id=$1",
            vec![id.into()],
        ))
        .await?;
        tx.commit().await?;
        self.metadata(scope, id).await
    }
    pub async fn complete(&self, scope: &str, id: Uuid) -> Result<File, AppError> {
        let tx = self.db.begin().await?;
        let row=tx.query_one_raw(sql("SELECT * FROM bracel_files WHERE id=$1 AND scope=$2 AND (status='available' OR (status IN ('pending','uploaded') AND expires_at>clock_timestamp())) FOR UPDATE",vec![id.into(),scope.into()])).await?.ok_or_else(missing)?;
        if row.try_get::<String>("", "status")? != "available" {
            let bytes = self.storage.get(scope, id).await.map_err(|_| invalid())?;
            validate_content(&row, &bytes, self.max_bytes)?;
            self.storage
                .put_at(scope, row.try_get("", "object_id")?, bytes)
                .await
                .map_err(|_| unavailable())?;
            tx.execute_raw(sql(
                "UPDATE bracel_files SET status='available' WHERE id=$1",
                vec![id.into()],
            ))
            .await?;
        }
        tx.commit().await?;
        let _ = self.storage.delete(scope, id).await;
        self.metadata(scope, id).await
    }
    pub async fn download(&self, scope: &str, id: Uuid) -> Result<(File, Vec<u8>), AppError> {
        let file = self.metadata(scope, id).await?;
        if file.status != "available" {
            return Err(missing());
        }
        let object=self.db.query_one_raw(sql("SELECT object_id FROM bracel_files WHERE id=$1 AND scope=$2 AND status='available'",vec![id.into(),scope.into()])).await?.ok_or_else(missing)?.try_get("","object_id")?;
        let bytes = self
            .storage
            .get(scope, object)
            .await
            .map_err(|_| unavailable())?;
        Ok((file, bytes))
    }
    pub async fn delete(&self, scope: &str, id: Uuid) -> Result<(), AppError> {
        let object: Uuid = self.db.query_one_raw(sql("UPDATE bracel_files SET status='deleted' WHERE id=$1 AND scope=$2 AND status<>'deleted' RETURNING object_id",vec![id.into(),scope.into()])).await?.ok_or_else(missing)?.try_get("", "object_id")?;
        // Retain the tombstone so failed physical deletion is retried by cleanup.
        let _ = self.storage.delete(scope, id).await;
        let _ = self.storage.delete(scope, object).await;
        Ok(())
    }
    pub async fn signed_url(
        &self,
        scope: &str,
        id: Uuid,
        upload: bool,
    ) -> Result<String, AppError> {
        let file = self.metadata(scope, id).await?;
        if if upload {
            file.status != "pending"
        } else {
            file.status != "available"
        } {
            return Err(missing());
        }
        let object = if upload {
            if self.db.query_one_raw(sql("SELECT id FROM bracel_files WHERE id=$1 AND scope=$2 AND expires_at>clock_timestamp()+interval '5 minutes'",vec![id.into(),scope.into()])).await?.is_none() { return Err(missing()); }
            id
        } else {
            self.db.query_one_raw(sql("SELECT object_id FROM bracel_files WHERE id=$1 AND scope=$2 AND status='available'",vec![id.into(),scope.into()])).await?.ok_or_else(missing)?.try_get("","object_id")?
        };
        self.storage
            .signed_url(scope, object, upload)
            .await
            .map_err(|_| unavailable())
    }
    pub async fn attach(
        &self,
        tx: &bracel::sea_orm::DatabaseTransaction,
        scope: &str,
        resource: Uuid,
        id: Uuid,
    ) -> Result<(), AppError> {
        let row=tx.query_one_raw(sql("SELECT id FROM bracel_files WHERE id=$1 AND scope=$2 AND status='available' FOR SHARE",vec![id.into(),scope.into()])).await?;
        if row.is_none() {
            return Err(missing());
        }
        tx.execute_raw(sql("INSERT INTO bracel_attachments(scope,resource,file_id) VALUES($1,$2,$3) ON CONFLICT DO NOTHING",vec![scope.into(),resource.into(),id.into()])).await?;
        Ok(())
    }
    pub async fn cleanup(&self) -> Result<u64, AppError> {
        let tx = self.db.begin().await?;
        let staging = tx.query_all_raw(sql("SELECT id,scope FROM bracel_files WHERE status='available' AND expires_at<=clock_timestamp() ORDER BY expires_at LIMIT 20 FOR UPDATE SKIP LOCKED",vec![])).await?;
        for row in staging {
            let id: Uuid = row.try_get("", "id")?;
            let scope: String = row.try_get("", "scope")?;
            if self.storage.delete(&scope, id).await.is_ok() {
                tx.execute_raw(sql(
                    "UPDATE bracel_files SET expires_at='infinity' WHERE id=$1",
                    vec![id.into()],
                ))
                .await?;
            }
        }
        let rows=tx.query_all_raw(sql("SELECT id,scope,object_id FROM bracel_files WHERE status='deleted' OR (status IN ('pending','uploaded') AND expires_at<=clock_timestamp()) ORDER BY expires_at LIMIT 20 FOR UPDATE SKIP LOCKED",vec![])).await?;
        let mut deleted = 0;
        for row in rows {
            let id: Uuid = row.try_get("", "id")?;
            let scope: String = row.try_get("", "scope")?;
            if self.storage.delete(&scope, id).await.is_err()
                || self
                    .storage
                    .delete(&scope, row.try_get("", "object_id")?)
                    .await
                    .is_err()
            {
                continue;
            }
            tx.execute_raw(sql(
                "DELETE FROM bracel_attachments WHERE file_id=$1",
                vec![id.into()],
            ))
            .await?;
            tx.execute_raw(sql("DELETE FROM bracel_files WHERE id=$1", vec![id.into()]))
                .await?;
            deleted += 1;
        }
        tx.commit().await?;
        Ok(deleted)
    }
}
fn validate_content(
    row: &bracel::sea_orm::QueryResult,
    bytes: &[u8],
    max: u64,
) -> Result<(), AppError> {
    if bytes.len() as u64 > max
        || bytes.len() as i64 != row.try_get::<i64>("", "size")?
        || format!("{:x}", Sha256::digest(bytes)) != row.try_get::<String>("", "sha256")?
    {
        return Err(invalid());
    }
    let accepted = match row.try_get::<String>("", "content_type")?.as_str() {
        "text/plain" => std::str::from_utf8(bytes).is_ok(),
        "image/png" => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        "image/jpeg" => bytes.starts_with(b"\xff\xd8\xff"),
        "application/pdf" => bytes.starts_with(b"%PDF-"),
        "application/octet-stream" => true,
        _ => false,
    };
    if accepted { Ok(()) } else { Err(invalid()) }
}

fn sql(query: &str, values: Vec<bracel::sea_orm::Value>) -> bracel::sea_orm::Statement {
    bracel::sea_orm::Statement::from_sql_and_values(
        bracel::sea_orm::DbBackend::Postgres,
        query,
        values,
    )
}
