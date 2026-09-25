mod support;
use axum::http::StatusCode;
use bracel_starter::{AppState, app, migrations::Migrator};
use sea_orm::{ConnectionTrait, DbBackend, Statement};
use sea_orm_migration::MigratorTrait;
use serde_json::{Value, json};
use support::{TestDb, note_title, request};

#[tokio::test]
async fn note_creation_participates_in_the_callers_transaction() {
    use bracel_starter::features::notes::{CreateNote, create_note};
    use sea_orm::TransactionTrait;
    let fixture = TestDb::new().await;
    Migrator::up(&fixture.db, None).await.unwrap();
    let transaction = fixture.db.begin().await.unwrap();
    let committed = create_note(
        &transaction,
        CreateNote {
            title: "committed".into(),
        },
    )
    .await
    .unwrap_or_else(|_| panic!("create in transaction"));
    assert!(note_title(&fixture.db, committed.id).await.is_none());
    transaction.commit().await.unwrap();
    assert_eq!(
        note_title(&fixture.db, committed.id).await.as_deref(),
        Some("committed")
    );
    let transaction = fixture.db.begin().await.unwrap();
    let rolled_back = create_note(
        &transaction,
        CreateNote {
            title: "rolled back".into(),
        },
    )
    .await
    .unwrap_or_else(|_| panic!("create in transaction"));
    assert!(
        transaction
            .execute_raw(Statement::from_sql_and_values(
                DbBackend::Postgres,
                "INSERT INTO notes (id, title) VALUES ($1, $2)",
                [rolled_back.id.into(), "duplicate".into()]
            ))
            .await
            .is_err()
    );
    transaction.rollback().await.unwrap();
    assert!(note_title(&fixture.db, rolled_back.id).await.is_none());
    assert!(note_title(&fixture.db, committed.id).await.is_some());
    fixture.cleanup().await;
}

#[tokio::test]
async fn diagnostics_inspect_history_without_mutating_and_handle_failures() {
    use bracel_starter::tooling::{diagnose, migration_status, offline};
    use sea_orm::TransactionTrait;
    let fixture = TestDb::new().await;
    let mut config = fixture.config.clone();
    let mut url = url::Url::parse(&config.database_url).unwrap();
    url.query_pairs_mut()
        .append_pair("options", &format!("-csearch_path={}", fixture.schema));
    config.database_url = url.into();

    let status = migration_status(&fixture.db).await.unwrap();
    assert_eq!(status.len(), 2);
    assert_eq!(status[0].status, "pending");
    let tables = fixture.db.query_one_raw(sea_orm::Statement::from_string(
        sea_orm::DbBackend::Postgres,
        "SELECT count(*)::bigint AS count FROM information_schema.tables WHERE table_schema = current_schema()"
    )).await.unwrap().unwrap();
    assert_eq!(
        tables.try_get::<i64>("", "count").unwrap(),
        0,
        "inspection must not create migration history"
    );
    let pending = diagnose(Ok(config.clone()), false, false, true).await;
    assert!(!pending.ok);
    assert_eq!(pending.database_status, "checked");
    assert!(
        pending
            .checks
            .iter()
            .any(|check| check.code == "DATABASE.PENDING_MIGRATIONS")
    );
    assert!(diagnose(Ok(config.clone()), true, false, true).await.ok);

    Migrator::up(&fixture.db, None).await.unwrap();
    let current = diagnose(Ok(config.clone()), false, false, true).await;
    assert!(current.ok);
    assert_eq!(current.migrations[0].status, "applied");
    // Compare inventory to the actual router for both configuration modes.
    for enabled in [false, true] {
        config.enable_example = enabled;
        let inventory = offline(Ok(&config), true, false).application.unwrap();
        let router = app(
            AppState {
                db: fixture.db.clone(),
            },
            &config,
        );
        for route in inventory["routes"].as_array().unwrap() {
            let template = route["path"].as_str().unwrap();
            // Invalid UUID gives 400 only when the get route is registered.
            let path = template.replace("{id}", "invalid");
            let (status, _, _) = request(
                &router,
                route["method"].as_str().unwrap(),
                &path,
                "{}",
                "application/json",
            )
            .await;
            assert_eq!(
                status != StatusCode::NOT_FOUND,
                route["enabled"].as_bool().unwrap()
            );
        }
    }
    let transaction = fixture.db.begin().await.unwrap();
    transaction
        .execute_unprepared("LOCK TABLE seaql_migrations IN ACCESS EXCLUSIVE MODE")
        .await
        .unwrap();
    let blocked = diagnose(Ok(config.clone()), true, false, true).await;
    assert!(!blocked.ok);
    assert!(
        blocked
            .checks
            .iter()
            .any(|check| check.code == "DATABASE.INSPECTION_FAILED")
    );
    transaction.rollback().await.unwrap();

    fixture.db.execute_unprepared("INSERT INTO seaql_migrations (version, applied_at) VALUES ('secret-sentinel-unknown-migration', 0)").await.unwrap();
    let incompatible = diagnose(Ok(config), true, false, true).await;
    assert!(!incompatible.ok);
    assert!(!incompatible.render(true).contains("secret-sentinel"));
    assert!(
        incompatible
            .checks
            .iter()
            .any(|check| check.code == "DATABASE.INSPECTION_FAILED")
    );
    fixture.cleanup().await;
}

#[tokio::test]
async fn postgres_http_contract_and_migration_lifecycle() {
    let fixture = TestDb::new().await;
    let router = app(
        AppState {
            db: fixture.db.clone(),
        },
        &fixture.config,
    );
    assert_eq!(
        request(&router, "GET", "/readyz", "", "application/json")
            .await
            .0,
        503
    );
    Migrator::up(&fixture.db, None).await.unwrap();
    Migrator::up(&fixture.db, None).await.unwrap();
    assert_eq!(
        request(&router, "GET", "/healthz", "", "application/json")
            .await
            .0,
        200
    );
    assert_eq!(
        request(&router, "GET", "/readyz", "", "application/json")
            .await
            .0,
        200
    );
    let (status, _, note) = request(
        &router,
        "POST",
        "/example/notes",
        r#"{"title":"  hello  "}"#,
        "application/json",
    )
    .await;
    assert_eq!(status, 201);
    assert_eq!(note["data"]["title"], "hello");
    let note_id = uuid::Uuid::parse_str(note["data"]["id"].as_str().unwrap()).unwrap();
    assert_eq!(note_id.get_version_num(), 7);
    let path = format!("/example/notes/{}", note["data"]["id"].as_str().unwrap());
    assert_eq!(
        request(&router, "GET", &path, "", "application/json")
            .await
            .2,
        note
    );
    assert_eq!(
        note_title(&fixture.db, note_id).await.as_deref(),
        Some("hello")
    );
    let legacy_id = uuid::Uuid::parse_str("95a73fe1-616e-4de6-b21e-74f8d9dfe638").unwrap();
    fixture
        .db
        .execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "INSERT INTO notes (id, title) VALUES ($1, $2)",
            [legacy_id.into(), "existing v4 note".into()],
        ))
        .await
        .unwrap();
    let (status, _, legacy_note) = request(
        &router,
        "GET",
        &format!("/example/notes/{legacy_id}"),
        "",
        "application/json",
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(legacy_note["data"]["id"], legacy_id.to_string());
    assert_eq!(legacy_note["data"]["title"], "existing v4 note");
    for (body, expected_issues) in [
        (
            "{}".to_owned(),
            json!([{"code": "invalid_type", "path": ["title"], "message": "The title field is required."}]),
        ),
        (
            r#"{"title":null}"#.to_owned(),
            json!([{"code": "invalid_type", "path": ["title"], "message": "The title field is required."}]),
        ),
        (
            r#"{"title":" \n "}"#.to_owned(),
            json!([{"code": "too_small", "path": ["title"], "message": "The title field is required."}]),
        ),
        (
            r#"{"title":12}"#.to_owned(),
            json!([{"code": "invalid_type", "path": ["title"], "message": "The title must be a string."}]),
        ),
        (
            r#"{"title":false}"#.to_owned(),
            json!([{"code": "invalid_type", "path": ["title"], "message": "The title must be a string."}]),
        ),
        (
            r#"{"title":[]}"#.to_owned(),
            json!([{"code": "invalid_type", "path": ["title"], "message": "The title must be a string."}]),
        ),
        (
            r#"{"title":{"secret":"sensitive-input"}}"#.to_owned(),
            json!([{"code": "invalid_type", "path": ["title"], "message": "The title must be a string."}]),
        ),
        (
            json!({"title": "é".repeat(201)}).to_string(),
            json!([{"code": "too_big", "path": ["title"], "message": "The title must not be greater than 200 characters."}]),
        ),
        (
            r#"{"title":"", "sensitive-input":"secret"}"#.to_owned(),
            json!([{"code": "unrecognized_keys", "path": [], "message": "Unknown fields are not allowed."}, {"code": "too_small", "path": ["title"], "message": "The title field is required."}]),
        ),
        (
            r#"{"title":"ok", "sensitive-input":"secret"}"#.to_owned(),
            json!([{"code": "unrecognized_keys", "path": [], "message": "Unknown fields are not allowed."}]),
        ),
        (
            r#"{"title":"first","title":"second"}"#.to_owned(),
            json!([{"code": "custom", "path": [], "message": "Expected an object with no duplicate fields."}]),
        ),
        (
            "[]".to_owned(),
            json!([{"code": "custom", "path": [], "message": "Expected an object with no duplicate fields."}]),
        ),
        (
            "null".to_owned(),
            json!([{"code": "custom", "path": [], "message": "Expected an object with no duplicate fields."}]),
        ),
    ] {
        let (status, _, problem) =
            request(&router, "POST", "/example/notes", &body, "application/json").await;
        assert_eq!(status, 422, "body: {body}");
        assert_eq!(problem["detail"], "The given data was invalid.");
        assert_eq!(problem["issues"], expected_issues, "body: {body}");
        assert!(problem.get("errors").is_none());
        assert!(!problem.to_string().contains("sensitive-input"));
    }
    let count = fixture
        .db
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT count(*)::bigint AS count FROM notes",
        ))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(count.try_get::<i64>("", "count").unwrap(), 2);
    for (body, content_type, expected) in [
        (r#"{"title":" "}"#, "application/json", 422),
        ("{", "application/json", 400),
        (r#"{"title":12}"#, "application/json", 422),
        (r#"{"title":"hi","unknown":true}"#, "application/json", 422),
        (r#"{"title":"hi"}"#, "text/plain", 415),
    ] {
        assert_eq!(
            request(&router, "POST", "/example/notes", body, content_type)
                .await
                .0,
            expected
        );
    }
    assert_eq!(
        request(
            &router,
            "POST",
            "/example/notes",
            &json!({"title":"x".repeat(20000)}).to_string(),
            "application/json"
        )
        .await
        .0,
        413
    );
    assert_eq!(
        request(
            &router,
            "GET",
            "/example/notes/not-a-uuid",
            "",
            "application/json"
        )
        .await
        .0,
        400
    );
    assert_eq!(
        request(
            &router,
            "GET",
            &format!("/example/notes/{}", uuid::Uuid::now_v7()),
            "",
            "application/json"
        )
        .await
        .0,
        404
    );
    assert_eq!(
        request(
            &router,
            "GET",
            "/missing?secret=hidden",
            "",
            "application/json"
        )
        .await
        .0,
        404
    );
    let (status, headers, _) = request(&router, "DELETE", &path, "", "application/json").await;
    assert_eq!(status, 405);
    assert!(headers.contains_key("allow"));
    let mut disabled = fixture.config.clone();
    disabled.enable_example = false;
    let disabled = app(
        AppState {
            db: fixture.db.clone(),
        },
        &disabled,
    );
    assert_eq!(
        request(
            &disabled,
            "POST",
            "/example/notes",
            "{}",
            "application/json"
        )
        .await
        .0,
        404
    );
    let spec: Value = serde_json::from_str(include_str!("../docs/openapi.json")).unwrap();
    assert_eq!(
        spec["components"]["schemas"]["Problem"]["properties"]["issues"]["items"]["$ref"],
        "#/components/schemas/ValidationIssue"
    );
    assert_eq!(
        spec["components"]["schemas"]["PathSegment"]["oneOf"][0]["type"],
        "string"
    );
    assert_eq!(
        spec["components"]["schemas"]["PathSegment"]["oneOf"][1]["type"],
        "integer"
    );
    assert_eq!(
        spec["paths"]["/example/notes"]["post"]["responses"]["201"]["content"]["application/json"]
            ["schema"]["$ref"],
        "#/components/schemas/Data_Note"
    );
    for field in spec["components"]["schemas"]["Note"]["required"]
        .as_array()
        .unwrap()
    {
        assert!(note["data"].get(field.as_str().unwrap()).is_some());
    }
    Migrator::down(&fixture.db, None).await.unwrap();
    assert_eq!(
        request(&router, "GET", &path, "", "application/json")
            .await
            .0,
        503
    );
    Migrator::up(&fixture.db, None).await.unwrap();
    assert_eq!(
        request(&router, "GET", &path, "", "application/json")
            .await
            .0,
        404
    );
    fixture.cleanup().await;
}

#[tokio::test]
async fn deadline_and_unavailable_database() {
    let mut fixture = TestDb::new().await;
    Migrator::up(&fixture.db, None).await.unwrap();
    fixture.config.request_timeout = std::time::Duration::from_millis(50);
    use sea_orm::TransactionTrait;
    let lock = fixture.db.begin().await.unwrap();
    lock.execute_unprepared("LOCK TABLE notes IN ACCESS EXCLUSIVE MODE")
        .await
        .unwrap();
    let router = app(
        AppState {
            db: fixture.db.clone(),
        },
        &fixture.config,
    );
    assert_eq!(
        request(
            &router,
            "POST",
            "/example/notes",
            r#"{"title":"blocked"}"#,
            "application/json"
        )
        .await
        .0,
        408
    );
    lock.rollback().await.unwrap();
    fixture.db.clone().close().await.unwrap();
    assert_eq!(
        request(&router, "GET", "/healthz", "", "application/json")
            .await
            .0,
        200
    );
    assert_eq!(
        request(&router, "GET", "/readyz", "", "application/json")
            .await
            .0,
        503
    );
    fixture.cleanup().await;
}
