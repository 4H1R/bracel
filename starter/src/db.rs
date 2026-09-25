use crate::config::Config;
use sea_orm::{ConnectOptions, Database, DatabaseConnection, DbErr};
use std::time::Duration;

pub async fn connect(config: &Config) -> Result<DatabaseConnection, DbErr> {
    let mut options = ConnectOptions::new(config.database_url.clone());
    options
        .max_connections(config.db_max_connections)
        .min_connections(1)
        .connect_timeout(Duration::from_secs(3))
        .acquire_timeout(Duration::from_secs(3))
        .idle_timeout(Duration::from_secs(300))
        .sqlx_logging(false);
    Database::connect(options).await
}
