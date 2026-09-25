use crate::{AppState, config, features::notes};
use axum::{Json, Router, extract::State, http::StatusCode};
pub use bracel::http::error;
pub use bracel::http::middleware;
pub use bracel::http::pagination;
pub use bracel::http::query;
pub use bracel::http::response;
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
    components(schemas(Health, notes::Note, notes::CreateNote, error::Problem)),
    info(
        title = "Bracel starter",
        description = "Example routes require ENABLE_EXAMPLE=true. AUTH_MODE=bearer requires access tokens and scopes; off permits anonymous local teaching. See docs/http.md."
    )
)]
struct Schemas;

pub struct ApiDoc;
impl OpenApi for ApiDoc {
    fn openapi() -> utoipa::openapi::OpenApi {
        registry(
            &bracel::config::Config::from_lookup(|_| None).expect("default HTTP config"),
            true,
        )
        .openapi()
    }
}

pub(crate) fn registry(
    config: &bracel::config::Config,
    examples: bool,
) -> bracel::http::registry::Registry<AppState> {
    use bracel::{
        http::registry::{Registry, RoutePolicy},
        utoipa_axum::routes,
    };
    let mut registry = Registry::new(config, Schemas::openapi());
    registry.register(routes!(health), RoutePolicy::Exempt, true, vec![]);
    registry.register(routes!(ready), RoutePolicy::Exempt, true, vec![]);
    notes::http::register(&mut registry, examples);
    crate::features::register(&mut registry);
    registry
}

pub fn app(state: AppState, config: &config::Config) -> Router {
    let mut http = config.http.clone();
    if config.machine_tokens {
        http.auth = http.auth.map(|auth| auth.with_tokens(state.db.clone()));
    }
    bracel::Application::new(http.clone())
        .merge(registry(&http, config.enable_example).into_router())
        .build(state)
}
