use crate::{AppState, config, features::notes};
use axum::{Extension, Json, Router, extract::State, http::StatusCode};
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

#[derive(Clone)]
struct Readiness {
    examples: bool,
    accounts: bool,
    tokens: bool,
}

#[utoipa::path(get, path = "/readyz", responses((status = 200, description = "Database and enabled account/example schemas are available", body = Data<Health>), (status = 408, description = "Configured request deadline exceeded", body = error::Problem, content_type = "application/problem+json"), (status = 503, description = "Database or required schema unavailable", body = error::Problem, content_type = "application/problem+json")))]
async fn ready(
    State(state): State<AppState>,
    Extension(checks): Extension<Readiness>,
) -> Result<Json<Data<Health>>, AppError> {
    use sea_orm::ConnectionTrait;
    tokio::time::timeout(
        Duration::from_secs(2),
        async {
            state.db.execute_unprepared("SELECT 1").await?;
            if checks.examples {
                state.db.execute_unprepared("SELECT id, title, created_at FROM notes LIMIT 0").await?;
            }
            if checks.tokens {
                state.db.execute_unprepared("SELECT id, token_hash, issuer, subject, scope, expires_at, revoked_at FROM bracel_tokens LIMIT 0").await?;
            }
            if checks.accounts {
                state.db.execute_unprepared("SELECT id, email, display_name, password_hash, email_verified_at FROM users LIMIT 0").await?;
                state.db.execute_unprepared("SELECT token_id, user_id FROM account_sessions LIMIT 0").await?;
                state.db.execute_unprepared("SELECT id, user_id, token_hash, expires_at, attempts, delivered, lease_id, lease_until, available_at FROM account_resets LIMIT 0").await?;
                state.db.execute_unprepared("SELECT id, user_id, email, token_hash, expires_at, attempts, delivered, lease_id, lease_until, available_at FROM account_verifications LIMIT 0").await?;
                state.db.execute_unprepared("SELECT key, attempts, expires_at FROM account_quotas LIMIT 0").await?;
            }
            Ok::<_, sea_orm::DbErr>(())
        },
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
        let http = bracel::config::Config::from_lookup(|_| None).expect("default HTTP config");
        Registrations::new(&http, None, true, true, true, true).openapi()
    }
}

/// A single composition of feature declarations for runtime and tooling.
pub struct Registrations {
    main: bracel::http::registry::Registry<AppState>,
    accounts: bracel::http::registry::Registry<AppState>,
    #[cfg(feature = "batteries")]
    batteries: bracel::http::registry::Registry<sea_orm::DatabaseConnection>,
}
impl Registrations {
    fn new(
        http: &bracel::config::Config,
        account_auth: Option<bracel::identity::BearerAuth>,
        examples: bool,
        accounts: bool,
        _batteries: bool,
        _files: bool,
    ) -> Self {
        let policies = middleware::Policies::new(http);
        let mut account_http = http.clone();
        account_http.auth = Some(account_auth.unwrap_or_else(|| {
            bracel::identity::BearerAuth::pending(crate::features::accounts::ISSUER, "starter-api")
                .expect("static account identity")
        }));
        Self {
            main: registry_with_policies(http, examples, policies.clone()),
            accounts: crate::features::accounts::registry_with_policies(
                &account_http,
                accounts,
                policies.clone(),
            ),
            #[cfg(feature = "batteries")]
            batteries: crate::batteries::registry_with_policies(
                http,
                _batteries,
                _batteries && _files,
                policies,
            ),
        }
    }
    pub fn configured(config: &config::Config) -> Self {
        Self::new(
            &config.http,
            None,
            config.enable_example,
            config.accounts.enabled,
            config.enable_batteries,
            config.providers.files_root.is_some(),
        )
    }
    pub fn inventory(&self) -> Vec<serde_json::Value> {
        let mut routes = self.main.inventory();
        routes.extend(self.accounts.inventory());
        #[cfg(feature = "batteries")]
        routes.extend(self.batteries.inventory());
        routes
    }
    pub fn openapi(&self) -> utoipa::openapi::OpenApi {
        let mut api = self.main.openapi();
        api.merge(self.accounts.openapi());
        #[cfg(feature = "batteries")]
        api.merge(self.batteries.openapi());
        api
    }
}

fn registry_with_policies(
    config: &bracel::config::Config,
    examples: bool,
    policies: middleware::Policies,
) -> bracel::http::registry::Registry<AppState> {
    use bracel::{
        http::registry::{Registry, RoutePolicy},
        utoipa_axum::routes,
    };
    let mut registry = Registry::with_policies(config, Schemas::openapi(), policies);
    registry.register(routes!(health), RoutePolicy::Exempt, true, vec![]);
    registry.register(routes!(ready), RoutePolicy::Exempt, true, vec![]);
    notes::http::register(&mut registry, examples);
    crate::features::register(&mut registry);
    registry
}

/// Convenience for tests and embedded callers. Production constructs Resources in bootstrap.
pub fn app(state: AppState, config: &config::Config) -> Router {
    let resources =
        crate::bootstrap::Resources::build(&state, config).expect("valid application resources");
    router(state, config, &resources)
}

/// Route assembly has no filesystem, environment or background-task effects.
pub fn router(
    state: AppState,
    config: &config::Config,
    _resources: &crate::bootstrap::Resources,
) -> Router {
    let mut http = config.http.clone();
    if config.machine_tokens || config.local_auth {
        http.auth = http.auth.map(|auth| auth.with_tokens(state.db.clone()));
    }
    let registrations = Registrations::new(
        &http,
        Some(crate::features::accounts::verifier(state.db.clone())),
        config.enable_example,
        config.accounts.enabled,
        config.enable_batteries,
        config.providers.files_root.is_some(),
    );
    let application = bracel::Application::new(http.clone())
        .merge(registrations.main.into_router())
        .merge(crate::features::accounts::decorate(
            registrations.accounts.into_router(),
            config.accounts.clone(),
        ));
    #[cfg(feature = "batteries")]
    let application = if config.enable_batteries {
        application.merge(crate::batteries::decorate(
            registrations.batteries.into_router(),
            state.db.clone(),
            &http,
            config.providers.clone(),
            _resources,
        ))
    } else {
        application
    };
    let router = application.build(state).layer(Extension(Readiness {
        examples: config.enable_example,
        accounts: config.accounts.enabled,
        tokens: config.accounts.enabled || config.machine_tokens || config.local_auth,
    }));
    #[cfg(feature = "telemetry")]
    let router = router.layer(axum::middleware::from_fn(
        bracel_integrations::telemetry::propagate,
    ));
    router
}

pub(crate) fn catalog_inventory() -> Vec<serde_json::Value> {
    let http = bracel::config::Config::from_lookup(|_| None).expect("default HTTP config");
    Registrations::new(&http, None, true, true, true, true).inventory()
}
