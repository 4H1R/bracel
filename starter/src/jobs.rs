//! Application job handlers. Register generated jobs in extensions.rs.
use bracel::jobs::{Failure, TypedJob, Worker};
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct CleanupAccounts {}
impl TypedJob for CleanupAccounts {
    const KIND: &'static str = "accounts.cleanup";
}

pub fn worker(db: DatabaseConnection) -> Worker {
    let mut worker = Worker::default();
    crate::extensions::jobs(&mut worker);
    worker
        .register_typed(move |_: CleanupAccounts| {
            let db = db.clone();
            async move {
                crate::features::accounts::cleanup(&db)
                    .await
                    .map_err(Failure::Retry)
            }
        })
        .expect("unique application job");
    worker
        .register("example.ping", 1, |payload| async move {
            if payload != serde_json::json!({}) {
                return Err(Failure::Permanent("invalid_payload"));
            }
            Ok(())
        })
        .expect("unique example job");
    worker
}
