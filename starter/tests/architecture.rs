use bracel::testing::{TestClient, TestDatabase};
use bracel_starter::{AppState, app, config::Config, migrations::Migrator};
use sea_orm::{ConnectionTrait, TransactionTrait};
use sea_orm_migration::MigratorTrait;

fn config(url: &str) -> Config {
    Config::from_lookup(|key| match key {
        "DATABASE_URL" => Some(url.into()),
        "ENABLE_EXAMPLE" => Some("true".into()),
        "AUTH_MODE" => Some("off".into()),
        _ => None,
    })
    .unwrap()
}

#[tokio::test]
async fn request_budget_is_shared_between_notes_and_accounts() {
    let url = std::env::var("TEST_DATABASE_URL").unwrap();
    let fixture = TestDatabase::connect(&url).await.unwrap();
    Migrator::up(&fixture.db, None).await.unwrap();
    let mut settings = config(&url);
    settings.http.anonymous_per_minute = 1;
    let client = TestClient::new(app(AppState::new(fixture.db.clone()), &settings));
    client
        .request("GET", "/example/notes", None)
        .await
        .assert_status(200);
    client
        .request("GET", "/api/users/me", None)
        .await
        .assert_status(429);
    fixture.cleanup().await.unwrap();
}

#[tokio::test]
async fn migration_history_keeps_optional_schema_and_legacy_data() {
    let url = std::env::var("TEST_DATABASE_URL").unwrap();
    let fixture = TestDatabase::connect(&url).await.unwrap();
    // A released default install already applied accounts after skipping 000004.
    struct ReleasedDefault;
    #[sea_orm_migration::async_trait::async_trait]
    impl MigratorTrait for ReleasedDefault {
        fn migrations() -> Vec<Box<dyn sea_orm_migration::MigrationTrait>> {
            Migrator::migrations()
                .into_iter()
                .filter(|migration| {
                    matches!(
                        migration.name(),
                        "m20260925_000001_create_notes"
                            | "m20260925_000002_note_created_at"
                            | "m20260926_000003_batteries"
                            | "m20260926_000005_accounts"
                    )
                })
                .collect()
        }
    }
    ReleasedDefault::up(&fixture.db, None).await.unwrap();
    fixture
        .db
        .execute_unprepared(
            "INSERT INTO notes(id,title) VALUES('00000000-0000-4000-8000-000000000001','legacy')",
        )
        .await
        .unwrap();
    Migrator::up(&fixture.db, None).await.unwrap();
    Migrator::up(&fixture.db, None).await.unwrap();
    assert!(
        Migrator::migrations()
            .iter()
            .any(|m| m.name() == "m20260926_000004_api_packages")
    );
    let tx = fixture.db.begin().await.unwrap();
    tx.execute_unprepared("SELECT id FROM projects LIMIT 0; SELECT queue FROM bracel_jobs LIMIT 0; SELECT name FROM bracel_calendar LIMIT 0").await.unwrap();
    tx.rollback().await.unwrap();
    let client = TestClient::new(app(AppState::new(fixture.db.clone()), &config(&url)));
    client
        .request(
            "GET",
            "/example/notes/00000000-0000-4000-8000-000000000001",
            None,
        )
        .await
        .assert_status(200);
    Migrator::down(&fixture.db, None).await.unwrap();
    Migrator::up(&fixture.db, None).await.unwrap();
    fixture.cleanup().await.unwrap();
}

#[tokio::test]
async fn lifecycle_drains_tasks_and_configuration_drives_inspection() {
    let mut lifecycle = bracel_starter::bootstrap::Lifecycle::default();
    let stop = lifecycle.subscribe();
    let completed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let task_completed = completed.clone();
    lifecycle.spawn(async move {
        bracel_starter::bootstrap::stopped(stop).await;
        task_completed.store(true, std::sync::atomic::Ordering::SeqCst);
    });
    lifecycle.listen();
    lifecycle.finish().await;
    assert!(completed.load(std::sync::atomic::Ordering::SeqCst));

    let settings = Config::from_lookup(|key| match key {
        "DATABASE_URL" => Some("postgres://localhost/test".into()),
        "AUTH_MODE" => Some("off".into()),
        "MAIL_SMTP_HOST" => Some("smtp.example.test".into()),
        "MAIL_SMTP_USERNAME" => Some("fixture".into()),
        "MAIL_SMTP_PASSWORD" => Some("fixture".into()),
        _ => None,
    })
    .unwrap();
    let registrations = bracel_starter::http::Registrations::configured(&settings);
    let routes = registrations.inventory();
    let me = routes
        .iter()
        .find(|r| r["path"] == "/api/users/me" && r["method"] == "GET")
        .unwrap();
    assert_eq!(me["authentication"], "bearer");
    let notes = routes
        .iter()
        .find(|r| r["path"] == "/example/notes")
        .unwrap();
    assert_eq!(notes["enabled"], false);
    let report = bracel_starter::cli::tooling::offline(Ok(&settings), true, false);
    let json: serde_json::Value = serde_json::from_str(&report.render(true)).unwrap();
    assert_eq!(
        json["application"]["capabilities"]["email"]["configured"],
        true
    );
}
