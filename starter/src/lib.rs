pub mod commands;
#[cfg(any(
    feature = "mail",
    feature = "storage",
    feature = "cache",
    feature = "outbound",
    feature = "telemetry"
))]
pub use bracel_integrations as integrations;
pub mod config;
pub mod db;
pub mod features;
pub mod http;
pub mod migrations;
pub use bracel::query;
pub mod tooling;

pub use http::{ApiDoc, app};

#[derive(Clone)]
pub struct AppState {
    pub db: sea_orm::DatabaseConnection,
}
