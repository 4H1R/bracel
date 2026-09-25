use axum::{
    Router,
    body::{Body, to_bytes},
    http::{HeaderMap, Request, StatusCode},
};
use sea_orm::{ConnectOptions, ConnectionTrait, Database, DatabaseConnection, DbErr};
use serde_json::Value;
use std::time::Duration;
use tower::ServiceExt;

pub struct TestDatabase {
    pub db: DatabaseConnection,
    pub admin: DatabaseConnection,
    pub schema: String,
}
impl TestDatabase {
    /// The caller must supply a disposable PostgreSQL database. Migrations remain application-owned.
    pub async fn connect(url: &str) -> Result<Self, DbErr> {
        let mut options = ConnectOptions::new(url);
        options
            .max_connections(2)
            .connect_timeout(Duration::from_secs(3))
            .sqlx_logging(false);
        let admin = Database::connect(options).await?;
        let schema = format!("test_{}", uuid::Uuid::now_v7().simple());
        admin
            .execute_unprepared(&format!("CREATE SCHEMA {schema}"))
            .await?;
        let mut options = ConnectOptions::new(url);
        options
            .max_connections(4)
            .connect_timeout(Duration::from_secs(3))
            .sqlx_logging(false)
            .set_schema_search_path(schema.clone());
        match Database::connect(options).await {
            Ok(db) => Ok(Self { db, admin, schema }),
            Err(error) => {
                admin
                    .execute_unprepared(&format!("DROP SCHEMA {schema} CASCADE"))
                    .await?;
                Err(error)
            }
        }
    }
    pub async fn cleanup(self) -> Result<(), DbErr> {
        self.db.close().await?;
        self.admin
            .execute_unprepared(&format!("DROP SCHEMA {} CASCADE", self.schema))
            .await?;
        self.admin.close().await
    }
}

#[derive(Clone)]
pub struct TestClient {
    router: Router,
    bearer: Option<String>,
}
pub struct TestResponse {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Value,
}
impl TestClient {
    pub fn new(router: Router) -> Self {
        Self {
            router,
            bearer: None,
        }
    }
    /// Exercises the actual authentication middleware using a signed or issued token.
    pub fn bearer(mut self, token: impl Into<String>) -> Self {
        self.bearer = Some(token.into());
        self
    }
    pub async fn request(&self, method: &str, path: &str, body: Option<Value>) -> TestResponse {
        let mut request = Request::builder().method(method).uri(path);
        if let Some(token) = &self.bearer {
            request = request.header("authorization", format!("Bearer {token}"));
        }
        if body.is_some() {
            request = request.header("content-type", "application/json");
        }
        let response = self
            .router
            .clone()
            .oneshot(
                request
                    .body(
                        body.map(|v| Body::from(v.to_string()))
                            .unwrap_or_else(Body::empty),
                    )
                    .unwrap(),
            )
            .await
            .unwrap();
        let (parts, body) = response.into_parts();
        let bytes = to_bytes(body, 1024 * 1024).await.unwrap();
        TestResponse {
            status: parts.status,
            headers: parts.headers,
            body: if bytes.is_empty() {
                Value::Null
            } else {
                serde_json::from_slice(&bytes).expect("JSON response")
            },
        }
    }
}
impl TestResponse {
    pub fn assert_status(&self, status: u16) -> &Self {
        assert_eq!(self.status.as_u16(), status, "{}", self.body);
        self
    }
    pub fn assert_issue(&self, field: &str, code: &str) -> &Self {
        self.assert_status(422);
        assert!(
            self.body["issues"]
                .as_array()
                .expect("issues")
                .iter()
                .any(|i| i["path"] == serde_json::json!([field]) && i["code"] == code)
        );
        self
    }
}

/// Unique fixture values without a shared process counter or database dependency.
pub fn unique(prefix: &str) -> String {
    format!("{prefix}-{}", uuid::Uuid::now_v7())
}
