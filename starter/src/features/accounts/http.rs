use super::{SCOPE, application, dto::*};
use crate::{
    AppState,
    http::{error::AppError, response::Data},
};
use axum::{
    Extension, Json, Router,
    extract::{Request, State},
    http::{HeaderMap, StatusCode, header},
    middleware::{self, Next},
    response::Response,
};
use bracel::{
    http::{
        extract::UniqueJson,
        registry::{Registry, RoutePolicy},
    },
    identity::Principal,
    utoipa_axum::routes,
};
use serde::de::DeserializeOwned;
use std::sync::Arc;
use tokio::sync::Semaphore;

#[derive(Clone)]
struct Runtime {
    settings: Settings,
    passwords: Arc<Semaphore>,
}
fn decode<T: DeserializeOwned>(value: serde_json::Value) -> Result<T, AppError> {
    serde_json::from_value(value).map_err(|_| {
        invalid(
            "",
            "Required fields must have the correct types; unknown fields are rejected.",
        )
    })
}
pub fn registry(config: &bracel::config::Config, enabled: bool) -> Registry<AppState> {
    let mut registry = Registry::new(config, utoipa::openapi::OpenApi::default());
    registry.register(routes!(register), RoutePolicy::Public, enabled, vec![]);
    registry.register(routes!(login), RoutePolicy::Public, enabled, vec![]);
    registry.register(routes!(forgot), RoutePolicy::Public, enabled, vec![]);
    registry.register(routes!(reset), RoutePolicy::Public, enabled, vec![]);
    registry.register(routes!(me), RoutePolicy::Scope(SCOPE), enabled, vec![]);
    registry.register(routes!(profile), RoutePolicy::Scope(SCOPE), enabled, vec![]);
    registry.register(routes!(logout), RoutePolicy::Scope(SCOPE), enabled, vec![]);
    registry.register(
        routes!(logout_all),
        RoutePolicy::Scope(SCOPE),
        enabled,
        vec![],
    );
    registry
}
pub fn router(
    db: sea_orm::DatabaseConnection,
    config: &bracel::config::Config,
    settings: Settings,
) -> Router<AppState> {
    let mut config = config.clone();
    config.auth = Some(super::verifier(db));
    registry(&config, settings.enabled)
        .into_router()
        .layer(Extension(Runtime {
            settings,
            passwords: Arc::new(Semaphore::new(4)),
        }))
        .layer(middleware::from_fn(no_store))
}
async fn no_store(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        "no-store".parse().expect("static header"),
    );
    response
}

#[utoipa::path(post,path="/api/auth/register",request_body=Registration,responses(
    (status=201,description="User and one-day bearer session",body=Data<Session>),
    (status=403,description="Registration disabled",body=crate::http::error::Problem,content_type="application/problem+json"),
    (status=409,description="Email already registered",body=crate::http::error::Problem,content_type="application/problem+json"),
    (status=422,description="Invalid account fields",body=crate::http::error::Problem,content_type="application/problem+json"),
    (status=429,description="Account quota exceeded",body=crate::http::error::Problem,content_type="application/problem+json"),
    (status=503,description="Account service unavailable",body=crate::http::error::Problem,content_type="application/problem+json")))]
async fn register(
    State(state): State<AppState>,
    Extension(runtime): Extension<Runtime>,
    UniqueJson(value): UniqueJson,
) -> Result<(StatusCode, Json<Data<Session>>), AppError> {
    if !runtime.settings.registration {
        return Err(AppError::new(
            StatusCode::FORBIDDEN,
            "Registration is disabled",
        ));
    }
    Ok((
        StatusCode::CREATED,
        Json(Data::new(
            application::register(&state.db, runtime.passwords, decode(value)?).await?,
        )),
    ))
}
#[utoipa::path(post,path="/api/auth/login",request_body=Login,responses(
    (status=200,description="One-day bearer session",body=Data<Session>),
    (status=401,description="Invalid credentials",body=crate::http::error::Problem,content_type="application/problem+json"),
    (status=422,description="Invalid fields",body=crate::http::error::Problem,content_type="application/problem+json"),
    (status=429,description="Account quota exceeded",body=crate::http::error::Problem,content_type="application/problem+json"),
    (status=503,description="Account service unavailable",body=crate::http::error::Problem,content_type="application/problem+json")))]
async fn login(
    State(state): State<AppState>,
    Extension(runtime): Extension<Runtime>,
    UniqueJson(value): UniqueJson,
) -> Result<Json<Data<Session>>, AppError> {
    Ok(Json(Data::new(
        application::login(&state.db, runtime.passwords, decode(value)?).await?,
    )))
}
#[utoipa::path(post,path="/api/auth/forgot-password",request_body=Forgot,responses(
    (status=202,description="Same response for known and unknown emails",body=Data<Accepted>),
    (status=422,description="Invalid email",body=crate::http::error::Problem,content_type="application/problem+json"),
    (status=429,description="Account quota exceeded",body=crate::http::error::Problem,content_type="application/problem+json"),
    (status=503,description="Mail or database unavailable",body=crate::http::error::Problem,content_type="application/problem+json")))]
async fn forgot(
    State(state): State<AppState>,
    Extension(runtime): Extension<Runtime>,
    UniqueJson(value): UniqueJson,
) -> Result<(StatusCode, Json<Data<Accepted>>), AppError> {
    if !runtime.settings.mail_configured {
        return Err(application::unavailable());
    }
    application::forgot(&state.db, decode(value)?).await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(Data::new(Accepted {
            message: "If that account exists, password reset instructions will be sent.",
        })),
    ))
}
#[utoipa::path(post,path="/api/auth/reset-password",request_body=Reset,responses(
    (status=204,description="Password replaced and all sessions revoked; log in again"),
    (status=400,description="Invalid or expired reset token",body=crate::http::error::Problem,content_type="application/problem+json"),
    (status=422,description="Invalid password",body=crate::http::error::Problem,content_type="application/problem+json"),
    (status=503,description="Account service unavailable",body=crate::http::error::Problem,content_type="application/problem+json")))]
async fn reset(
    State(state): State<AppState>,
    Extension(runtime): Extension<Runtime>,
    UniqueJson(value): UniqueJson,
) -> Result<StatusCode, AppError> {
    application::reset(&state.db, runtime.passwords, decode(value)?).await?;
    Ok(StatusCode::NO_CONTENT)
}
#[utoipa::path(get,path="/api/users/me",responses(
    (status=200,description="Current user, without private persistence fields",body=Data<User>),
    (status=401,description="Missing or invalid local session",body=crate::http::error::Problem,content_type="application/problem+json")))]
async fn me(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
) -> Result<Json<Data<User>>, AppError> {
    Ok(Json(Data::new(
        application::me(&state.db, &principal).await?,
    )))
}
#[utoipa::path(patch,path="/api/users/me",request_body=Profile,responses(
    (status=200,description="Update current user's display name",body=Data<User>),
    (status=401,description="Missing or invalid local session",body=crate::http::error::Problem,content_type="application/problem+json"),
    (status=422,description="Invalid profile",body=crate::http::error::Problem,content_type="application/problem+json")))]
async fn profile(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    UniqueJson(value): UniqueJson,
) -> Result<Json<Data<User>>, AppError> {
    Ok(Json(Data::new(
        application::profile(&state.db, &principal, decode(value)?).await?,
    )))
}
fn credential(headers: &HeaderMap) -> Result<&str, AppError> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split_once(' '))
        .map(|(_, v)| v)
        .ok_or_else(application::denied)
}
#[utoipa::path(post,path="/api/auth/logout",responses(
    (status=204,description="Current session revoked"),
    (status=401,description="Missing or invalid local session",body=crate::http::error::Problem,content_type="application/problem+json")))]
async fn logout(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    headers: HeaderMap,
) -> Result<StatusCode, AppError> {
    application::logout(&state.db, &principal, credential(&headers)?, false).await?;
    Ok(StatusCode::NO_CONTENT)
}
#[utoipa::path(post,path="/api/auth/logout-all",responses(
    (status=204,description="All account sessions revoked"),
    (status=401,description="Missing or invalid local session",body=crate::http::error::Problem,content_type="application/problem+json")))]
async fn logout_all(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    headers: HeaderMap,
) -> Result<StatusCode, AppError> {
    application::logout(&state.db, &principal, credential(&headers)?, true).await?;
    Ok(StatusCode::NO_CONTENT)
}
