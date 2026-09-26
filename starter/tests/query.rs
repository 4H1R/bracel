mod support;
use bracel_starter::{
    AppState, app,
    features::notes::{self, CreateNote},
    migrations::Migrator,
};
use sea_orm::{ConnectionTrait, DbBackend, Statement};
use sea_orm_migration::MigratorTrait;
use serde_json::Value;
use support::{TestDb, note_title, request};

async fn list(router: &axum::Router, query: &str) -> Value {
    let (status, _, value) = request(
        router,
        "GET",
        &format!("/example/notes?{query}"),
        "",
        "application/json",
    )
    .await;
    assert_eq!(status, 200, "{value}");
    value
}

#[tokio::test]
async fn filters_sort_and_cursor_binding_use_the_real_database() {
    let fixture = TestDb::new().await;
    Migrator::up(&fixture.db, None).await.unwrap();
    let mut ids = Vec::new();
    for title in [
        "a%_\\ note",
        "aZZZ note",
        "meeting alpha",
        "meeting beta",
        "other",
    ] {
        let note = notes::create_note(
            &fixture.db,
            CreateNote {
                title: title.into(),
            },
        )
        .await
        .ok()
        .unwrap();
        ids.push(note.id);
        assert_eq!(
            note_title(&fixture.db, note.id).await.as_deref(),
            Some(title)
        );
    }
    fixture
        .db
        .execute_unprepared("UPDATE notes SET created_at='2026-09-25T00:00:00.123456Z'")
        .await
        .unwrap();
    let router = app(AppState::new(fixture.db.clone()), &fixture.config);
    assert_eq!(
        list(&router, "filter[title]=%25_%5C").await["data"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        list(&router, "filter[title_exact]=meeting+alpha").await["data"][0]["id"],
        ids[2].to_string()
    );
    assert_eq!(
        list(&router, &format!("filter[id]={}", ids[1])).await["data"][0]["id"],
        ids[1].to_string()
    );
    assert_eq!(
        list(&router, "filter[title]=%27+OR+1%3D1--").await["data"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    assert_eq!(
        list(&router, "filter[created_after]=2026-09-26T00:00:00Z").await["data"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    assert_eq!(
        list(&router, "filter[created_before]=2026-09-24T00:00:00Z").await["data"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    let first = list(&router, "filter[title]=meeting&sort=created_at&limit=1").await;
    assert_eq!(first["data"][0]["id"], ids[2].to_string());
    let cursor = first["page"]["next_cursor"].as_str().unwrap();
    let second = list(
        &router,
        &format!("after={cursor}&limit=2&sort=created_at&filter[title]=meeting"),
    )
    .await;
    assert_eq!(second["data"].as_array().unwrap().len(), 1);
    assert_eq!(second["data"][0]["id"], ids[3].to_string());
    for query in [
        format!("after={cursor}&filter[title]=other&sort=created_at"),
        format!("after={cursor}&filter[title]=meeting&sort=-created_at"),
    ] {
        assert_eq!(
            request(
                &router,
                "GET",
                &format!("/example/notes?{query}"),
                "",
                "application/json"
            )
            .await
            .0,
            422
        );
    }
    // Equivalent timestamps and parameter order canonicalize to the same scope.
    let first = list(
        &router,
        "filter[created_after]=2026-09-25T00:00:00Z&limit=1",
    )
    .await;
    let token = first["page"]["next_cursor"].as_str().unwrap();
    assert_eq!(
        list(
            &router,
            &format!("after={token}&filter[created_after]=2026-09-25T01:00:00%2B01:00")
        )
        .await["data"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    for (query, expected) in [
        ("filter[id]=secret-sentinel", 422),
        ("filter[title]=", 422),
        ("filter[created_after]=2026-09-25T00:00:00.1234567Z", 422),
        ("filter[owner]=secret-sentinel", 400),
        ("include=owner", 400),
        ("filter[title]=one&filter%5Btitle%5D=two", 400),
        ("filter[title]=%XX", 400),
        ("filter[title]=%FF", 400),
        ("sort=created_at,id", 400),
    ] {
        let (status, _, value) = request(
            &router,
            "GET",
            &format!("/example/notes?{query}"),
            "",
            "application/json",
        )
        .await;
        assert_eq!(status, expected, "{query}");
        assert!(!value.to_string().contains("secret-sentinel"));
    }
    // Deleting the next matching row gives an empty page without changing scope.
    fixture
        .db
        .execute_raw(Statement::from_sql_and_values(
            DbBackend::Postgres,
            "DELETE FROM notes WHERE id=$1",
            [ids[3].into()],
        ))
        .await
        .unwrap();
    assert!(
        list(
            &router,
            &format!("after={cursor}&filter[title]=meeting&sort=created_at")
        )
        .await["data"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    fixture.cleanup().await;
}
