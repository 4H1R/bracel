use bracel::{
    jobs::{self, Failure, JobSpec},
    testing::TestDatabase,
};
use sea_orm::{ConnectionTrait, TransactionTrait};
use serde_json::json;
use std::time::Duration;
async fn database() -> TestDatabase {
    let db = TestDatabase::connect(&std::env::var("TEST_DATABASE_URL").unwrap())
        .await
        .unwrap();
    db.db.execute_unprepared(jobs::SCHEMA).await.unwrap();
    db
}
fn spec(key: &str) -> JobSpec {
    JobSpec {
        kind: "test".into(),
        version: 1,
        payload: json!({"id":1}),
        dedupe_key: key.into(),
        max_attempts: 2,
    }
}
#[tokio::test]
async fn transactional_intent_deduplication_and_concurrent_claims() {
    let db = database().await;
    assert!(
        jobs::enqueue(&db.db, &spec("schedule:reserved"))
            .await
            .is_err()
    );
    let transaction = db.db.begin().await.unwrap();
    jobs::enqueue(&transaction, &spec("rollback"))
        .await
        .unwrap();
    assert!(
        jobs::claim(&db.db, Duration::from_secs(5))
            .await
            .unwrap()
            .is_none()
    );
    transaction.rollback().await.unwrap();
    assert!(
        jobs::claim(&db.db, Duration::from_secs(5))
            .await
            .unwrap()
            .is_none()
    );
    let id = jobs::enqueue(&db.db, &spec("one")).await.unwrap();
    assert_eq!(id, jobs::enqueue(&db.db, &spec("one")).await.unwrap());
    let mut conflict = spec("one");
    conflict.payload = json!({});
    assert!(jobs::enqueue(&db.db, &conflict).await.is_err());
    let (a, b) = tokio::join!(
        jobs::claim(&db.db, Duration::from_secs(5)),
        jobs::claim(&db.db, Duration::from_secs(5))
    );
    let jobs: Vec<_> = [a.unwrap(), b.unwrap()].into_iter().flatten().collect();
    assert_eq!(jobs.len(), 1);
    assert!(jobs::complete(&db.db, &jobs[0]).await.unwrap());
    assert!(!jobs::complete(&db.db, &jobs[0]).await.unwrap());
    db.cleanup().await.unwrap();
}
#[tokio::test]
async fn expired_lease_cannot_ack_reclaimed_job_and_replay_is_explicit() {
    let db = database().await;
    let id = jobs::enqueue(&db.db, &spec("lease")).await.unwrap();
    let old = jobs::claim(&db.db, Duration::from_secs(2))
        .await
        .unwrap()
        .unwrap();
    db.db
        .execute_unprepared(
            "UPDATE bracel_jobs SET lease_until=clock_timestamp()-interval '1 second'",
        )
        .await
        .unwrap();
    let current = jobs::claim(&db.db, Duration::from_secs(5))
        .await
        .unwrap()
        .unwrap();
    assert!(!jobs::complete(&db.db, &old).await.unwrap());
    assert!(
        !jobs::fail(&db.db, &old, Failure::Permanent("stale"))
            .await
            .unwrap()
    );
    assert!(
        jobs::fail(&db.db, &current, Failure::Retry("temporary"))
            .await
            .unwrap()
    );
    assert_eq!(jobs::failed(&db.db).await.unwrap().len(), 1);
    assert!(
        jobs::replay(&db.db, id, "fixed test dependency")
            .await
            .unwrap()
    );
    assert!(!jobs::replay(&db.db, id, "duplicate").await.unwrap());
    let mut worker = jobs::Worker::default();
    worker.register("test", 1, |_| async { Ok(()) }).unwrap();
    assert!(worker.tick(&db.db, Duration::from_secs(1)).await.unwrap());
    assert!(!worker.tick(&db.db, Duration::from_secs(1)).await.unwrap());
    db.cleanup().await.unwrap();
}
#[tokio::test]
async fn schedules_coalesce_and_enqueue_atomically_across_replicas() {
    let db = database().await;
    jobs::schedule(&db.db, "test", 60, &spec("unused"))
        .await
        .unwrap();
    db.db
        .execute_unprepared(
            "UPDATE bracel_schedules SET next_due_at=clock_timestamp()-interval '1 hour'",
        )
        .await
        .unwrap();
    let (a, b) = tokio::join!(jobs::tick_schedules(&db.db), jobs::tick_schedules(&db.db));
    assert_eq!(a.unwrap() + b.unwrap(), 1);
    let job = jobs::claim(&db.db, Duration::from_secs(5))
        .await
        .unwrap()
        .unwrap();
    assert!(jobs::complete(&db.db, &job).await.unwrap());
    assert!(
        jobs::claim(&db.db, Duration::from_secs(5))
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(jobs::tick_schedules(&db.db).await.unwrap(), 0);
    jobs::enable_schedule(&db.db, "test", false).await.unwrap();
    db.db
        .execute_unprepared(
            "UPDATE bracel_schedules SET next_due_at=clock_timestamp()-interval '1 hour'",
        )
        .await
        .unwrap();
    assert_eq!(jobs::tick_schedules(&db.db).await.unwrap(), 0);
    db.cleanup().await.unwrap();
}

#[tokio::test]
async fn application_commands_share_context_and_validate_arguments() {
    let db = database().await;
    let commands = bracel_starter::cli::commands::registry();
    assert!(commands.manifest().iter().any(|c| c.name == "jobs:work"));
    assert!(
        commands
            .run("jobs:retry", vec![], db.db.clone())
            .await
            .is_err()
    );
    assert!(
        commands
            .run("unknown", vec![], db.db.clone())
            .await
            .is_err()
    );
    commands
        .run("db:seed", vec![], db.db.clone())
        .await
        .unwrap();
    commands
        .run("db:seed", vec![], db.db.clone())
        .await
        .unwrap();
    assert_eq!(
        commands
            .run("jobs:once", vec![], db.db.clone())
            .await
            .unwrap()["processed"],
        true
    );
    assert_eq!(
        commands
            .run("jobs:once", vec![], db.db.clone())
            .await
            .unwrap()["processed"],
        false
    );
    db.cleanup().await.unwrap();
}
