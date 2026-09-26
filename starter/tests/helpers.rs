use axum::{extract::FromRequestParts, http::Request};
use bracel::{identity::Principal, jobs, testing::TestDatabase};
use bracel_starter::{AppState, features::accounts::CurrentUser, migrations::Migrator, schedules};
use sea_orm::ConnectionTrait;
use sea_orm_migration::MigratorTrait;

#[tokio::test]
async fn default_migrations_schedules_and_worker_form_one_workflow() {
    let fixture = TestDatabase::connect(&std::env::var("TEST_DATABASE_URL").unwrap())
        .await
        .unwrap();
    // Upgrade a database from immediately before the scheduling migration.
    let preceding = Migrator::migrations()
        .iter()
        .position(|migration| migration.name() == "m20260927_000006_job_scheduling")
        .unwrap();
    Migrator::up(&fixture.db, Some(preceding as u32))
        .await
        .unwrap();
    Migrator::up(&fixture.db, None).await.unwrap();
    Migrator::up(&fixture.db, None).await.unwrap();
    schedules::sync(&fixture.db).await.unwrap();
    fixture.db.execute_unprepared("INSERT INTO bracel_calendar SELECT 'app.removed',expression,timezone,kind,version,payload,max_attempts,next_due_at,last_job,true FROM bracel_calendar WHERE name='app.accounts.cleanup'; INSERT INTO bracel_calendar SELECT 'operator.custom',expression,timezone,kind,version,payload,max_attempts,next_due_at,last_job,true FROM bracel_calendar WHERE name='app.accounts.cleanup';").await.unwrap();
    schedules::sync(&fixture.db).await.unwrap();
    let rows = fixture.db.query_all_raw(sea_orm::Statement::from_string(sea_orm::DbBackend::Postgres, "SELECT name,enabled FROM bracel_calendar WHERE name<>'app.accounts.cleanup' ORDER BY name".to_string())).await.unwrap();
    assert!(!rows[0].try_get::<bool>("", "enabled").unwrap());
    assert!(rows[1].try_get::<bool>("", "enabled").unwrap());
    fixture
        .db
        .execute_unprepared("DELETE FROM bracel_calendar WHERE name<>'app.accounts.cleanup'")
        .await
        .unwrap();
    // Enabling the optional package migration later must tolerate these objects.
    fixture
        .db
        .execute_unprepared(jobs::UPGRADE_QUEUES)
        .await
        .unwrap();
    fixture
        .db
        .execute_unprepared(
            "UPDATE bracel_calendar SET next_due_at=clock_timestamp()-interval '1 hour'",
        )
        .await
        .unwrap();
    schedules::sync(&fixture.db).await.unwrap();
    assert_eq!(schedules::tick(&fixture.db).await.unwrap(), 1);
    fixture
        .db
        .execute_unprepared(
            "UPDATE bracel_calendar SET next_due_at=clock_timestamp()-interval '1 hour'",
        )
        .await
        .unwrap();
    schedules::tick(&fixture.db).await.unwrap();
    let row = fixture
        .db
        .query_one_raw(sea_orm::Statement::from_string(
            sea_orm::DbBackend::Postgres,
            "SELECT count(*)::bigint AS n FROM bracel_jobs".to_string(),
        ))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.try_get::<i64>("", "n").unwrap(), 1);
    assert!(
        bracel_starter::jobs::worker(fixture.db.clone())
            .tick(&fixture.db, std::time::Duration::from_secs(25))
            .await
            .unwrap()
    );
    assert!(
        !bracel_starter::jobs::worker(fixture.db.clone())
            .tick(&fixture.db, std::time::Duration::from_secs(25))
            .await
            .unwrap()
    );
    let commands = bracel_starter::cli::commands::registry();
    assert!(commands.contains("jobs:parallel"));
    assert!(commands.contains("schedule:sync"));
    assert!(
        jobs::calendar(
            &fixture.db,
            "invalid",
            "invalid",
            "UTC",
            &jobs::JobSpec {
                kind: "test".into(),
                version: 1,
                payload: serde_json::json!({}),
                dedupe_key: "test".into(),
                max_attempts: 1
            }
        )
        .await
        .is_err()
    );
    fixture.cleanup().await.unwrap();
}

#[tokio::test]
async fn current_user_rejects_missing_external_and_unknown_principals() {
    let fixture = TestDatabase::connect(&std::env::var("TEST_DATABASE_URL").unwrap())
        .await
        .unwrap();
    Migrator::up(&fixture.db, None).await.unwrap();
    let state = AppState::new(fixture.db.clone());
    #[cfg(feature = "cache")]
    {
        state
            .cache
            .put("test", "shared", b"value".to_vec())
            .await
            .unwrap();
        assert_eq!(
            state.clone().cache.get("test", "shared").await,
            Some(b"value".to_vec())
        );
        assert!(
            bracel_starter::config::Config::from_lookup(|key| match key {
                "DATABASE_URL" => Some("postgres://localhost/starter".into()),
                "CACHE_MAX_ITEM_BYTES" => Some("0".into()),
                _ => None,
            })
            .is_err()
        );
    }
    for principal in [
        None,
        Some(
            Principal::new(
                "external".into(),
                uuid::Uuid::now_v7().to_string(),
                "account:self".into(),
            )
            .unwrap(),
        ),
        Some(
            Principal::new(
                "bracel-starter:accounts".into(),
                uuid::Uuid::now_v7().to_string(),
                "account:self".into(),
            )
            .unwrap(),
        ),
    ] {
        let (mut parts, _) = Request::new(()).into_parts();
        if let Some(principal) = principal {
            parts.extensions.insert(principal);
        }
        assert!(
            CurrentUser::from_request_parts(&mut parts, &state)
                .await
                .is_err()
        );
    }
    drop(state);
    fixture.cleanup().await.unwrap();
}
