mod contracts;
use crate::{AppState, config, features::notes};
use axum::{Json, Router, extract::State, http::StatusCode, routing::get};
pub use bracel::http::error;
pub use bracel::http::middleware;
pub use bracel::http::pagination;
pub use bracel::http::query;
pub use bracel::http::response;
use contracts::Contracts;
use error::AppError;
use response::Data;
use serde::Serialize;
use std::time::Duration;
use utoipa::{OpenApi, ToSchema};

#[derive(Serialize, ToSchema)]
pub struct Health {
    pub status: String,
}

#[utoipa::path(get, path = "/healthz", responses((status = 200, description = "Process alive; no dependencies checked", body = Data<Health>)))]
async fn health() -> Json<Data<Health>> {
    Json(Data::new(Health {
        status: "ok".into(),
    }))
}

#[utoipa::path(get, path = "/readyz", responses((status = 200, description = "Database query succeeds", body = Data<Health>), (status = 408, description = "Configured request deadline exceeded", body = error::Problem, content_type = "application/problem+json"), (status = 503, description = "Database unavailable", body = error::Problem, content_type = "application/problem+json")))]
async fn ready(State(state): State<AppState>) -> Result<Json<Data<Health>>, AppError> {
    use sea_orm::ConnectionTrait;
    tokio::time::timeout(
        Duration::from_secs(2),
        state
            .db
            .execute_unprepared("SELECT id, title, created_at FROM notes LIMIT 0"),
    )
    .await
    .map_err(|_| {
        AppError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "Database readiness deadline exceeded",
        )
    })??;
    Ok(Json(Data::new(Health {
        status: "ready".into(),
    })))
}

#[derive(OpenApi)]
#[openapi(
    modifiers(&Contracts),
    paths(
        health,
        ready,
        notes::http::create,
        notes::http::get_note,
        notes::http::list
    ),
    components(schemas(Health, notes::Note, notes::CreateNote, error::Problem)),
    info(
        title = "Bracel starter",
        version = "0.1.0",
        description = "Example routes require ENABLE_EXAMPLE=true. AUTH_MODE=bearer requires access tokens and scopes; off permits anonymous local teaching. See docs/http.md."
    )
)]
pub struct ApiDoc;

pub fn app(state: AppState, config: &config::Config) -> Router {
    let mut router = Router::new()
        .route("/healthz", get(health))
        .route("/readyz", get(ready));
    if config.enable_example {
        router = router.merge(notes::http::routes(
            &middleware::Policies::new(config),
            config.auth.is_some(),
        ));
    }
    bracel::Application::new(config.http.clone())
        .merge(router)
        .build(state)
}
