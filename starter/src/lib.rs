#[cfg(feature = "batteries")]
pub mod batteries;
#[cfg(feature = "cache")]
pub mod cache;
pub mod cli;
pub mod extensions;
pub mod jobs;
pub mod middleware;
pub mod schedules;
#[cfg(any(
    feature = "mail",
    feature = "storage",
    feature = "cache",
    feature = "outbound",
    feature = "telemetry"
))]
pub use bracel_integrations as integrations;
pub mod bootstrap;
pub mod config;
pub mod db;
pub mod features;
pub mod http;
pub mod migrations;
pub mod provider_settings;
pub use bracel::query;

pub use http::{ApiDoc, app};

#[derive(Clone)]
pub struct AppState {
    pub db: sea_orm::DatabaseConnection,
    #[cfg(feature = "cache")]
    pub cache: cache::ScopedCache,
}

impl AppState {
    pub fn new(db: sea_orm::DatabaseConnection) -> Self {
        Self {
            db,
            #[cfg(feature = "cache")]
            cache: cache::Settings::default().build(),
        }
    }

    pub fn from_config(db: sea_orm::DatabaseConnection, _config: &config::Config) -> Self {
        Self {
            db,
            #[cfg(feature = "cache")]
            cache: _config.cache.build(),
        }
    }
}
