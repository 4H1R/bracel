use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use bracel_starter::config::Config;
use sea_orm::{ConnectionTrait, DatabaseConnection};
use serde_json::Value;
use tower::ServiceExt;

pub async fn request(
    app: &Router,
    method: &str,
    path: &str,
    body: &str,
    content_type: &str,
) -> (StatusCode, axum::http::HeaderMap, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header("content-type", content_type)
                .header("x-request-id", "untrusted-secret")
                .body(Body::from(body.to_owned()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = to_bytes(response.into_body(), 65536).await.unwrap();
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    if status.is_success() {
        assert_eq!(headers["content-type"], "application/json");
        assert!(
            value.get("data").is_some(),
            "JSON successes use the shared data envelope"
        );
    }
    let request_id = uuid::Uuid::parse_str(headers["x-request-id"].to_str().unwrap()).unwrap();
    assert_eq!(request_id.get_version_num(), 7);
    if status.is_client_error() || status.is_server_error() {
        assert_eq!(headers["content-type"], "application/problem+json");
        assert_eq!(value["status"], status.as_u16());
        assert_eq!(value["type"], "about:blank");
        assert_eq!(
            value["request_id"],
            headers["x-request-id"].to_str().unwrap()
        );
        assert!(!value.to_string().contains("untrusted-secret"));
        if status != StatusCode::UNPROCESSABLE_ENTITY {
            assert!(value.get("issues").is_none());
        }
    }
    (status, headers, value)
}

pub async fn note_title(db: &impl ConnectionTrait, id: uuid::Uuid) -> Option<String> {
    db.query_one_raw(sea_orm::Statement::from_sql_and_values(
        sea_orm::DbBackend::Postgres,
        "SELECT title FROM notes WHERE id = $1",
        [id.into()],
    ))
    .await
    .unwrap()
    .map(|row| row.try_get("", "title").unwrap())
}

pub struct TestDb {
    pub admin: DatabaseConnection,
    pub db: DatabaseConnection,
    pub schema: String,
    pub config: Config,
}
impl TestDb {
    pub async fn new() -> Self {
        let url = std::env::var("TEST_DATABASE_URL")
            .expect("TEST_DATABASE_URL is mandatory; tests never silently skip PostgreSQL");
        let config = Config::from_lookup(|key| match key {
            "DATABASE_URL" => Some(url.clone()),
            "ENABLE_EXAMPLE" => Some("true".into()),
            _ => None,
        })
        .unwrap();
        let bracel::testing::TestDatabase { admin, db, schema } =
            bracel::testing::TestDatabase::connect(&url).await.unwrap();
        Self {
            admin,
            db,
            schema,
            config,
        }
    }
    pub async fn cleanup(self) {
        self.db.close().await.unwrap();
        self.admin
            .execute_unprepared(&format!("DROP SCHEMA {} CASCADE", self.schema))
            .await
            .unwrap();
        self.admin.close().await.unwrap();
    }
}
