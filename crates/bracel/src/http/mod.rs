pub mod error;
pub mod middleware;
pub mod pagination;
pub mod query;
pub mod response;

use crate::config::Config;
use axum::{Router, http::StatusCode};
use error::AppError;

/// Assemble all routes before building. Register custom middleware with Axum
/// layers on the supplied router; Bracel's context and error handling wrap it.
/// Health/readiness and route authorization are explicit application choices.
pub struct Application<S = ()> {
    router: Router<S>,
    config: Config,
}

impl<S: Clone + Send + Sync + 'static> Application<S> {
    pub fn new(config: Config) -> Self {
        Self {
            router: Router::new(),
            config,
        }
    }

    pub fn merge(mut self, routes: Router<S>) -> Self {
        self.router = self.router.merge(routes);
        self
    }

    /// Apply common error responses, request IDs, logs, CORS, body limits and
    /// deadlines once, after every feature and its route policies are registered.
    pub fn build(self, state: S) -> Router {
        let router = self
            .router
            .fallback(|| async { AppError::new(StatusCode::NOT_FOUND, "Route not found") })
            .method_not_allowed_fallback(|| async {
                AppError::new(StatusCode::METHOD_NOT_ALLOWED, "Method not allowed")
            });
        middleware::common(router, &self.config).with_state(state)
    }
}
