mod support;

use axum::Router;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use bracel_starter::{AppState, app, migrations::Migrator};
use sea_orm::{ConnectionTrait, DbBackend, Statement};
use sea_orm_migration::MigratorTrait;
use serde_json::{Value, json};
use support::{TestDb, note_title, request};
use uuid::Uuid;

async fn page(router: &Router, query: &str) -> Value {
    let (status, _, value) = request(
        router,
        "GET",
        &format!("/example/notes{query}"),
        "",
        "application/json",
    )
    .await;
    assert_eq!(status, 200, "{value}");
    value
}

#[tokio::test]
async fn cursor_pages_cover_ties_boundaries_deleted_anchor_and_new_inserts() {
    let fixture = TestDb::new().await;
    Migrator::up(&fixture.db, None).await.unwrap();
    let router = app(AppState::new(fixture.db.clone()), &fixture.config);
    let empty = page(&router, "").await;
    let spec: Value = serde_json::from_str(include_str!("../docs/openapi.json")).unwrap();
    assert_eq!(
        spec["paths"]["/example/notes"]["get"]["responses"]["200"]["content"]["application/json"]["schema"]
            ["$ref"],
        "#/components/schemas/Page_Note"
    );
    for field in spec["components"]["schemas"]["PageInfo"]["required"]
        .as_array()
        .unwrap()
    {
        assert!(empty["page"].get(field.as_str().unwrap()).is_some());
    }
    assert_eq!(
        empty,
        json!({"data": [], "page": {"limit": 25, "has_more": false, "next_cursor": null}})
    );
    // Mixed UUID versions, all with the same microsecond timestamp.
    let ids = [
        "01998c9e-8000-7000-8000-000000000001",
        "01998c9e-8000-7000-8000-000000000002",
        "95a73fe1-616e-4de6-b21e-74f8d9dfe638",
        "ffffffff-ffff-4fff-8fff-ffffffffffff",
    ];
    for id in ids {
        fixture.db.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,
            "INSERT INTO notes (id, title, created_at) VALUES ($1, $2, '2026-09-25T00:00:00.123456Z')",
            [Uuid::parse_str(id).unwrap().into(), "tied".into()]
        )).await.unwrap();
    }
    let first = page(&router, "?limit=2").await;
    assert_eq!(first["data"][0]["id"], ids[3]);
    assert_eq!(first["data"][1]["id"], ids[2]);
    assert_eq!(first["page"]["has_more"], true);
    let cursor = first["page"]["next_cursor"].as_str().unwrap();
    let second = page(&router, &format!("?limit=2&after={cursor}")).await;
    assert_eq!(second["data"][0]["id"], ids[1]);
    assert_eq!(second["data"][1]["id"], ids[0]);
    assert_eq!(second["page"]["has_more"], false);
    assert!(second["page"]["next_cursor"].is_null());
    // Deleting the anchor does not invalidate its position; newer rows do not
    // shift the next page as they would with offset pagination.
    fixture
        .db
        .execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "DELETE FROM notes WHERE id = $1",
            [Uuid::parse_str(ids[2]).unwrap().into()],
        ))
        .await
        .unwrap();
    let (status, _, created) = request(
        &router,
        "POST",
        "/example/notes",
        r#"{"title":"newer"}"#,
        "application/json",
    )
    .await;
    assert_eq!(status, 201);
    assert_eq!(
        page(&router, &format!("?limit=2&after={cursor}")).await,
        second
    );
    assert_eq!(page(&router, "?limit=1").await["data"][0], created["data"]);

    // A cursor at the final row produces an empty page, not an error.
    let mut payload: Value =
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(cursor).unwrap()).unwrap();
    payload["position"]["id"] = json!(ids[0]);
    let last = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&payload).unwrap());
    assert_eq!(
        page(&router, &format!("?after={last}")).await["data"],
        json!([])
    );
    // Limits are actually enforced, including the default and maximum.
    fixture.db.execute_unprepared("INSERT INTO notes (id, title) SELECT md5(n::text)::uuid, 'bulk' FROM generate_series(1, 105) n").await.unwrap();
    assert_eq!(
        page(&router, "").await["data"].as_array().unwrap().len(),
        25
    );
    assert_eq!(
        page(&router, "?limit=100").await["data"]
            .as_array()
            .unwrap()
            .len(),
        100
    );

    let mut config = fixture.config.clone();
    config.enable_example = false;
    let disabled = app(AppState::new(fixture.db.clone()), &config);
    assert_eq!(
        request(
            &disabled,
            "GET",
            "/example/notes?limit=2",
            "",
            "application/json"
        )
        .await
        .0,
        404
    );
    fixture.cleanup().await;
}

#[tokio::test]
async fn invalid_pagination_is_safe_even_when_database_is_unavailable() {
    let fixture = TestDb::new().await;
    Migrator::up(&fixture.db, None).await.unwrap();
    let router = app(AppState::new(fixture.db.clone()), &fixture.config);
    fixture.db.clone().close().await.unwrap();
    for (query, expected) in [
        ("?limit=0", 422),
        ("?limit=101", 422),
        ("?limit=-1", 400),
        ("?limit=18446744073709551616", 400),
        ("?limit=secret-sentinel", 400),
        ("?limit=1&limit=2", 400),
        ("?after=a&after=b", 400),
        ("?sort=title", 400),
        ("?filter=secret-sentinel", 400),
        ("?after=", 422),
        ("?after=secret-sentinel", 422),
    ] {
        let (status, _, problem) = request(
            &router,
            "GET",
            &format!("/example/notes{query}"),
            "",
            "application/json",
        )
        .await;
        assert_eq!(status, expected, "{query}");
        assert!(!problem.to_string().contains("secret-sentinel"));
    }
    let input = bracel_starter::features::notes::ListNotes::parse("", "public")
        .ok()
        .unwrap();
    let valid = json!({"version": 1, "scope": input.cursor_scope(), "position": {"created_at": "2026-09-25T00:00:00.123456Z", "id": "01998c9e-8000-7000-8000-000000000001"}});
    let mut invalid = Vec::new();
    for (key, value) in [
        ("version", json!(2)),
        ("scope", json!("other:secret-sentinel")),
        ("unknown", json!(true)),
    ] {
        let mut payload = valid.clone();
        payload[key] = value;
        invalid.push(payload);
    }
    for (key, value) in [
        ("id", json!("invalid")),
        ("created_at", json!("invalid")),
        ("created_at", json!("2026-09-25T00:00:00.123456789Z")),
        ("created_at", json!("0000-01-01T00:00:00Z")),
        ("unknown", json!(true)),
    ] {
        let mut payload = valid.clone();
        payload["position"][key] = value;
        invalid.push(payload);
    }
    let mut tokens: Vec<String> = invalid
        .into_iter()
        .map(|payload| URL_SAFE_NO_PAD.encode(serde_json::to_vec(&payload).unwrap()))
        .collect();
    tokens.push("a".repeat(1025));
    tokens.push(URL_SAFE_NO_PAD.encode(b"not-json"));
    for token in tokens {
        let (status, _, problem) = request(
            &router,
            "GET",
            &format!("/example/notes?after={token}"),
            "",
            "application/json",
        )
        .await;
        assert_eq!(status, 422);
        assert_eq!(problem["issues"][0]["path"], json!(["after"]));
        assert!(!problem.to_string().contains("secret-sentinel"));
    }
    assert_eq!(
        request(&router, "GET", "/example/notes", "", "application/json")
            .await
            .0,
        503
    );
    fixture.cleanup().await;
}

#[tokio::test]
async fn upgrade_preserves_legacy_rows_and_timestamp_index() {
    let fixture = TestDb::new().await;
    Migrator::up(&fixture.db, Some(1)).await.unwrap();
    let old_router = app(AppState::new(fixture.db.clone()), &fixture.config);
    assert_eq!(
        request(&old_router, "GET", "/readyz", "", "application/json")
            .await
            .0,
        503
    );
    let id = Uuid::parse_str("95a73fe1-616e-4de6-b21e-74f8d9dfe638").unwrap();
    fixture
        .db
        .execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "INSERT INTO notes (id, title) VALUES ($1, 'legacy')",
            [id.into()],
        ))
        .await
        .unwrap();
    Migrator::up(&fixture.db, None).await.unwrap();
    Migrator::up(&fixture.db, None).await.unwrap();
    assert_eq!(note_title(&fixture.db, id).await.as_deref(), Some("legacy"));
    let router = app(AppState::new(fixture.db.clone()), &fixture.config);
    let listing = page(&router, "").await;
    assert_eq!(listing["data"][0]["id"], id.to_string());
    assert!(
        listing["data"][0]["created_at"]
            .as_str()
            .unwrap()
            .parse::<sea_orm::prelude::DateTimeUtc>()
            .is_ok()
    );
    let index = fixture.db.query_one_raw(Statement::from_string(DbBackend::Postgres,
        "SELECT indexdef FROM pg_indexes WHERE schemaname = current_schema() AND indexname = 'notes_created_at_id_idx'"
    )).await.unwrap().unwrap();
    assert!(
        index
            .try_get::<String>("", "indexdef")
            .unwrap()
            .contains("created_at DESC, id DESC")
    );
    Migrator::down(&fixture.db, Some(1)).await.unwrap();
    assert_eq!(note_title(&fixture.db, id).await.as_deref(), Some("legacy"));
    Migrator::up(&fixture.db, None).await.unwrap();
    assert_eq!(page(&router, "").await["data"][0]["id"], id.to_string());
    fixture.cleanup().await;
}
