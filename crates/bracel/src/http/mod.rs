pub mod error;
pub mod middleware;
pub mod pagination;
pub mod query;
pub mod response;

use crate::config::Config;
use axum::{Router, http::StatusCode};
use error::AppError;

/// Builds an application's routes inside Bracel's common HTTP middleware.
/// Apply feature middleware and authorization before merging each router.
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

    /// Attach application state and wrap the completed router in common middleware.
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
