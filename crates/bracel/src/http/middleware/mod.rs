//! Reusable route policies; construct once and clone shared handles across features.
mod auth;
mod proxy;
mod rate_limit;
mod request_context;
pub use request_context::RequestContext;

use crate::{config::Config, http::error::AppError, identity::BearerAuth};
use axum::{
    Router,
    extract::{ConnectInfo, Request, State},
    http::{Method, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
};
use rate_limit::{Denied, Limiter};
use std::{net::SocketAddr, sync::Arc};
use tokio::sync::Semaphore;
use tower_http::cors::CorsLayer;

#[derive(Clone, Copy)]
pub enum Access {
    Public,
    Scope(&'static str),
}

struct Shared {
    auth: Option<BearerAuth>,
    anonymous: Limiter,
    authenticated: Limiter,
    writes: Limiter,
    concurrency: Semaphore,
    trusted_proxies: Vec<ipnet::IpNet>,
}

#[derive(Clone)]
pub struct Policies(Arc<Shared>);

impl Policies {
    pub fn new(config: &Config) -> Self {
        Self(Arc::new(Shared {
            auth: config.auth.clone(),
            anonymous: Limiter::new(config.anonymous_per_minute, config.rate_max_keys),
            authenticated: Limiter::new(config.authenticated_per_minute, config.rate_max_keys),
            writes: Limiter::new(config.writes_per_minute, config.rate_max_keys),
            concurrency: Semaphore::new(config.max_in_flight),
            trusted_proxies: config.trusted_proxies.clone(),
        }))
    }
    /// Attach after registering routes. Protected policies fail closed without a verifier.
    pub fn apply<S: Clone + Send + Sync + 'static>(
        &self,
        router: Router<S>,
        access: Access,
    ) -> Router<S> {
        router.route_layer(middleware::from_fn_with_state(
            (self.clone(), access),
            guard,
        ))
    }
}

fn rate_error(denied: Denied) -> Response {
    let (status, wait, detail) = match denied {
        Denied::Quota(wait) => (StatusCode::TOO_MANY_REQUESTS, wait, "Request rate exceeded"),
        Denied::Capacity => (
            StatusCode::SERVICE_UNAVAILABLE,
            60,
            "Request capacity unavailable",
        ),
    };
    (
        [(header::RETRY_AFTER, wait.to_string())],
        AppError::new(status, detail),
    )
        .into_response()
}

async fn guard(
    State((policies, access)): State<(Policies, Access)>,
    mut request: Request,
    next: Next,
) -> Response {
    // Forwarded addresses are accepted only from explicitly trusted peers.
    let peer = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|peer| {
            proxy::client_ip(peer.0.ip(), request.headers(), &policies.0.trusted_proxies)
                .to_string()
        })
        .unwrap_or_else(|| "unknown-peer".into());
    if let Err(error) = policies.0.anonymous.check(peer.clone()) {
        return rate_error(error);
    }
    let Ok(_permit) = policies.0.concurrency.try_acquire() else {
        return (
            [(header::RETRY_AFTER, "1")],
            AppError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "Too many concurrent requests",
            ),
        )
            .into_response();
    };
    let key = if let Access::Scope(scope) = access {
        let Some(verifier) = &policies.0.auth else {
            return AppError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "Authentication unavailable",
            )
            .into_response();
        };
        let principal = match auth::authenticate(request.headers(), verifier, scope).await {
            Ok(p) => p,
            Err(error) => return error.into_response(),
        };
        let key = principal.cursor_scope();
        if let Err(error) = policies.0.authenticated.check(key.clone()) {
            return rate_error(error);
        }
        request.extensions_mut().insert(principal);
        format!("principal:{key}")
    } else {
        format!("peer:{peer}")
    };
    if !matches!(
        *request.method(),
        Method::GET | Method::HEAD | Method::OPTIONS
    ) && let Err(error) = policies.0.writes.check(key)
    {
        return rate_error(error);
    }
    let mut response = next.run(request).await;
    if matches!(access, Access::Scope(_)) {
        response
            .headers_mut()
            .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    }
    response
}

/// Outermost context observes CORS, deadline, extractor and route-policy failures.
pub(crate) fn common<S: Clone + Send + Sync + 'static>(
    router: Router<S>,
    config: &Config,
) -> Router<S> {
    let cors = CorsLayer::new()
        .allow_origin(config.cors_origins.clone())
        .allow_methods([
            Method::GET,
            Method::HEAD,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([
            header::AUTHORIZATION,
            header::CONTENT_TYPE,
            header::IF_MATCH,
            header::HeaderName::from_static("idempotency-key"),
            header::HeaderName::from_static("last-event-id"),
            header::HeaderName::from_static("traceparent"),
            header::HeaderName::from_static("tracestate"),
        ])
        .expose_headers([
            header::RETRY_AFTER,
            header::HeaderName::from_static("x-request-id"),
        ]);
    #[cfg(feature = "compression")]
    let router = if config.compression {
        router.layer(tower_http::compression::CompressionLayer::new())
    } else {
        router
    };
    router
        .layer(axum::extract::DefaultBodyLimit::max(config.body_limit))
        .layer(middleware::from_fn_with_state(
            config.request_timeout,
            request_context::deadline,
        ))
        .layer(cors)
        .layer(middleware::from_fn(request_context::request_context))
        .layer(axum::Extension(config.metrics.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, routing::get};
    use std::{
        sync::atomic::{AtomicUsize, Ordering},
        time::Duration,
    };
    use tower::ServiceExt;

    #[tokio::test]
    async fn concurrency_rejects_without_queueing_and_timeout_releases_permit() {
        let mut config =
            Config::from_lookup(|k| (k == "DATABASE_URL").then(|| "postgres://local/db".into()))
                .unwrap();
        config.max_in_flight = 1;
        config.request_timeout = Duration::from_millis(100);
        config.cors_origins = vec!["https://client.example".parse().unwrap()];
        let entered = Arc::new(tokio::sync::Notify::new());
        let count = Arc::new(AtomicUsize::new(0));
        let handler = {
            let entered = entered.clone();
            let count = count.clone();
            move || {
                let entered = entered.clone();
                let count = count.clone();
                async move {
                    if count.fetch_add(1, Ordering::SeqCst) == 0 {
                        entered.notify_one();
                        std::future::pending::<()>().await;
                    }
                    "ok"
                }
            }
        };
        let policies = Policies::new(&config);
        let router = common(
            policies.apply(Router::new().route("/work", get(handler)), Access::Public),
            &config,
        )
        .with_state(());
        let first = tokio::spawn(
            router.clone().oneshot(
                Request::builder()
                    .uri("/work")
                    .header("origin", "https://client.example")
                    .body(Body::empty())
                    .unwrap(),
            ),
        );
        entered.notified().await;
        let second = router
            .clone()
            .oneshot(Request::builder().uri("/work").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(second.status(), 503);
        assert_eq!(second.headers()[header::RETRY_AFTER], "1");
        let timeout = first.await.unwrap().unwrap();
        assert_eq!(timeout.status(), 408);
        assert_eq!(
            timeout.headers()["access-control-allow-origin"],
            "https://client.example"
        );
        assert_eq!(
            timeout.headers()[header::CONTENT_TYPE],
            "application/problem+json"
        );
        assert!(timeout.headers().contains_key("x-request-id"));
        assert_eq!(
            router
                .oneshot(Request::builder().uri("/work").body(Body::empty()).unwrap())
                .await
                .unwrap()
                .status(),
            200
        );
        let router = common(
            policies.apply(
                Router::new().route("/private", get(|| async { "no" })),
                Access::Scope("read"),
            ),
            &config,
        )
        .with_state(());
        assert_eq!(
            router
                .oneshot(
                    Request::builder()
                        .uri("/private")
                        .body(Body::empty())
                        .unwrap()
                )
                .await
                .unwrap()
                .status(),
            503
        );
    }
}
