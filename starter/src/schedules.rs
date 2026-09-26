//! Application schedules. Run schedule:sync after changing definitions, then
//! keep schedule:work and jobs:work running as separate processes.
use bracel::jobs::{self, JobSpec, TypedJob};
use sea_orm::{ConnectionTrait, DatabaseConnection, DbErr, TransactionTrait};

pub struct Schedule {
    pub name: &'static str,
    pub expression: &'static str,
    pub timezone: &'static str,
    pub job: JobSpec,
}

pub fn definitions() -> Vec<Schedule> {
    vec![Schedule {
        name: "app.accounts.cleanup",
        expression: "0 0 * * * *",
        timezone: "UTC",
        job: JobSpec {
            kind: crate::jobs::CleanupAccounts::KIND.into(),
            version: crate::jobs::CleanupAccounts::VERSION,
            payload: serde_json::json!({}),
            dedupe_key: "accounts.cleanup".into(),
            max_attempts: 5,
        },
    }]
}

pub async fn sync(db: &DatabaseConnection) -> Result<(), DbErr> {
    let tx = db.begin().await?;
    tx.execute_unprepared("UPDATE bracel_calendar SET enabled=false WHERE name LIKE 'app.%'")
        .await?;
    for schedule in definitions() {
        if !schedule.name.starts_with("app.") {
            return Err(DbErr::Custom(
                "Application schedule names must start with app.".into(),
            ));
        }
        jobs::calendar(
            &tx,
            schedule.name,
            schedule.expression,
            schedule.timezone,
            &schedule.job,
        )
        .await?;
    }
    tx.commit().await
}

pub async fn tick(db: &DatabaseConnection) -> Result<u64, DbErr> {
    Ok(jobs::tick_schedules(db).await? + jobs::tick_calendar(db).await?)
}

pub async fn run(
    db: &DatabaseConnection,
    mut stop: tokio::sync::watch::Receiver<bool>,
) -> Result<(), DbErr> {
    while !*stop.borrow() {
        tick(db).await?;
        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => {},
            _ = stop.changed() => break,
        }
    }
    Ok(())
}
