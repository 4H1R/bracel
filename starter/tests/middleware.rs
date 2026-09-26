mod support;
use axum::{
    Router,
    body::{Body, to_bytes},
    http::{HeaderMap, Request, StatusCode},
};
use bracel_starter::{
    AppState, app,
    config::Config,
    features::{
        identity::BearerAuth,
        notes::{self, CreateNote},
    },
    migrations::Migrator,
};
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use sea_orm_migration::MigratorTrait;
use serde_json::{Value, json};
use support::{TestDb, note_title};
use tower::ServiceExt;

fn token(mut claims: Value) -> String {
    if claims.get("exp").is_none() {
        claims["exp"] = json!(jsonwebtoken::get_current_timestamp() + 300);
    }
    let mut header = Header::new(Algorithm::RS256);
    header.typ = Some("at+jwt".into());
    encode(
        &header,
        &claims,
        &EncodingKey::from_rsa_pem(include_bytes!("fixtures/test-only-private.pem")).unwrap(),
    )
    .unwrap()
}
fn claims(sub: &str, scope: &str) -> Value {
    json!({"iss":"https://issuer.example","aud":"starter-api","sub":sub,"scope":scope})
}
fn verifier() -> BearerAuth {
    BearerAuth::new(
        include_str!("fixtures/test-only-public.pem"),
        "https://issuer.example",
        "starter-api",
    )
    .unwrap()
}

async fn send(
    router: &Router,
    method: &str,
    path: &str,
    token: Option<&str>,
    extras: &[(&str, &str)],
) -> (StatusCode, HeaderMap, Value) {
    let mut builder = Request::builder().method(method).uri(path);
    if let Some(token) = token {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    for (key, value) in extras {
        builder = builder.header(*key, *value);
    }
    let response = router
        .clone()
        .oneshot(builder.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    assert!(headers.contains_key("x-request-id"));
    assert_eq!(headers["x-content-type-options"], "nosniff");
    let body = to_bytes(response.into_body(), 65536).await.unwrap();
    let value = if body.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&body).unwrap()
    };
    if status.is_client_error() || status.is_server_error() {
        assert_eq!(headers["content-type"], "application/problem+json");
        assert_eq!(
            value["request_id"],
            headers["x-request-id"].to_str().unwrap()
        );
        assert!(!value.to_string().contains("secret-sentinel"));
    }
    (status, headers, value)
}

#[tokio::test]
async fn bearer_scope_cors_and_cursor_contracts() {
    let fixture = TestDb::new().await;
    Migrator::up(&fixture.db, None).await.unwrap();
    for title in ["one", "two"] {
        let note = notes::create_note(
            &fixture.db,
            CreateNote {
                title: title.into(),
            },
        )
        .await
        .ok()
        .unwrap();
        assert_eq!(
            note_title(&fixture.db, note.id).await.as_deref(),
            Some(title)
        );
    }
    let mut config = fixture.config.clone();
    config.http.auth = Some(verifier());
    config.http.cors_origins = vec!["https://client.example".parse().unwrap()];
    let router = app(AppState::new(fixture.db.clone()), &config);
    assert_eq!(
        support::request(&router, "GET", "/healthz", "", "application/json")
            .await
            .0,
        200
    );
    let origin = [("origin", "https://client.example")];
    let (status, headers, _) = send(&router, "GET", "/example/notes", None, &origin).await;
    assert_eq!(status, 401);
    assert_eq!(headers["www-authenticate"], "Bearer");
    assert_eq!(
        headers["access-control-allow-origin"],
        "https://client.example"
    );
    assert_eq!(send(&router, "GET", "/missing", None, &[]).await.0, 404);
    assert_eq!(send(&router, "GET", "/healthz", None, &[]).await.0, 200);
    let (status, headers, _) = send(
        &router,
        "OPTIONS",
        "/example/notes",
        None,
        &[
            ("origin", "https://client.example"),
            ("access-control-request-method", "POST"),
            (
                "access-control-request-headers",
                "authorization,content-type",
            ),
        ],
    )
    .await;
    assert_eq!(status, 200);
    assert!(headers.contains_key("access-control-allow-origin"));
    assert!(!headers.contains_key("access-control-allow-credentials"));
    let read = token(claims("alice", "notes:read"));
    let (status, headers, page) = send(
        &router,
        "GET",
        "/example/notes?limit=1",
        Some(&read),
        &origin,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(headers["cache-control"], "no-store");
    assert_eq!(
        send(&router, "POST", "/example/notes", Some(&read), &[])
            .await
            .0,
        403
    );
    let write = token(claims("alice", "notes:write"));
    assert_eq!(
        send(&router, "GET", "/example/notes", Some(&write), &[])
            .await
            .0,
        403
    );
    assert_eq!(
        send(&router, "POST", "/example/notes", Some(&write), &[])
            .await
            .0,
        415
    );
    let bob = token(claims("bob", "notes:read"));
    let cursor = page["page"]["next_cursor"].as_str().unwrap();
    assert_eq!(
        send(
            &router,
            "GET",
            &format!("/example/notes?after={cursor}"),
            Some(&bob),
            &[]
        )
        .await
        .0,
        422
    );
    assert_eq!(
        send(
            &router,
            "GET",
            &format!("/example/notes?after={cursor}"),
            Some(&read),
            &[]
        )
        .await
        .0,
        200
    );
    for (name, value) in [
        ("iss", json!("https://wrong.example")),
        ("aud", json!("wrong")),
        ("exp", json!(1)),
        ("exp", Value::Null),
        ("sub", json!("")),
        ("nbf", json!(jsonwebtoken::get_current_timestamp() + 3600)),
    ] {
        let mut value_claims = claims("alice", "notes:read");
        value_claims[name] = value;
        assert_eq!(
            send(
                &router,
                "GET",
                "/example/notes",
                Some(&token(value_claims)),
                &[]
            )
            .await
            .0,
            401,
            "{name}"
        );
    }
    let mut wrong_header = Header::new(Algorithm::HS256);
    wrong_header.typ = Some("at+jwt".into());
    let wrong_algorithm = encode(
        &wrong_header,
        &claims("alice", "notes:read"),
        &EncodingKey::from_secret(b"secret-sentinel"),
    )
    .unwrap();
    // Modifying signed claims must fail signature validation.
    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
    let parts: Vec<_> = read.split('.').collect();
    let mut forged: Value =
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[1]).unwrap()).unwrap();
    forged["sub"] = json!("mallory");
    let forged = format!(
        "{}.{}.{}",
        parts[0],
        URL_SAFE_NO_PAD.encode(serde_json::to_vec(&forged).unwrap()),
        parts[2]
    );
    assert_eq!(
        send(&router, "GET", "/example/notes", Some(&forged), &[])
            .await
            .0,
        401
    );
    for missing in ["iss", "aud", "sub"] {
        let mut value = claims("alice", "notes:read");
        value.as_object_mut().unwrap().remove(missing);
        assert_eq!(
            send(&router, "GET", "/example/notes", Some(&token(value)), &[])
                .await
                .0,
            401
        );
    }
    let mut jwt_header = Header::new(Algorithm::RS256);
    let mut value = claims("alice", "notes:read");
    value["exp"] = json!(jsonwebtoken::get_current_timestamp() + 300);
    let key = EncodingKey::from_rsa_pem(include_bytes!("fixtures/test-only-private.pem")).unwrap();
    assert_eq!(
        send(
            &router,
            "GET",
            "/example/notes",
            Some(&encode(&jwt_header, &value, &key).unwrap()),
            &[]
        )
        .await
        .0,
        401
    );
    jwt_header.typ = Some("at+jwt".into());
    jwt_header.crit = Some(vec!["unsupported".into()]);
    assert_eq!(
        send(
            &router,
            "GET",
            "/example/notes",
            Some(&encode(&jwt_header, &value, &key).unwrap()),
            &[]
        )
        .await
        .0,
        401
    );
    jwt_header.crit = None;
    value.as_object_mut().unwrap().remove("exp");
    assert_eq!(
        send(
            &router,
            "GET",
            "/example/notes",
            Some(&encode(&jwt_header, &value, &key).unwrap()),
            &[]
        )
        .await
        .0,
        401
    );
    for invalid in [&wrong_algorithm, "secret-sentinel"] {
        assert_eq!(
            send(&router, "GET", "/example/notes", Some(invalid), &[])
                .await
                .0,
            401
        );
    }
    assert_eq!(
        send(
            &router,
            "GET",
            "/example/notes",
            Some(&read),
            &[("authorization", "Bearer secret-sentinel")]
        )
        .await
        .0,
        401
    );
    let (_, headers, _) = send(
        &router,
        "GET",
        "/example/notes",
        Some(&read),
        &[("origin", "https://other.example")],
    )
    .await;
    assert!(!headers.contains_key("access-control-allow-origin"));
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/example/notes")
                .header("authorization", format!("Bearer {write}"))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"title":"authorized write"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 201);
    let inventory = bracel_starter::cli::tooling::offline(Ok(&config), true, false)
        .application
        .unwrap();
    let route = inventory["routes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["method"] == "GET" && r["path"] == "/example/notes")
        .unwrap();
    assert_eq!(route["authentication"], "bearer");
    assert_eq!(route["required_scope"], "notes:read");
    assert!(
        route["query_parameters"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["name"] == "filter[title]")
    );
    assert!(!inventory.to_string().contains("BEGIN PUBLIC KEY"));
    fixture.cleanup().await;
}

#[tokio::test]
async fn named_quotas_preserve_headers_exempt_health_and_ignore_forwarded_ip() {
    let fixture = TestDb::new().await;
    Migrator::up(&fixture.db, None).await.unwrap();
    let mut config = fixture.config.clone();
    config.http.anonymous_per_minute = 1;
    let router = app(AppState::new(fixture.db.clone()), &config);
    assert_eq!(
        send(&router, "GET", "/example/notes", None, &[]).await.0,
        200
    );
    let (status, headers, _) = send(
        &router,
        "GET",
        "/example/notes",
        None,
        &[("x-forwarded-for", "198.51.100.2")],
    )
    .await;
    assert_eq!(status, 429);
    assert!(
        headers["retry-after"]
            .to_str()
            .unwrap()
            .parse::<u64>()
            .unwrap()
            > 0
    );
    assert_eq!(send(&router, "GET", "/healthz", None, &[]).await.0, 200);
    config.http.anonymous_per_minute = 120;
    config.http.authenticated_per_minute = 1;
    config.http.auth = Some(verifier());
    let router = app(AppState::new(fixture.db.clone()), &config);
    let alice = token(claims("alice", "notes:read"));
    let bob = token(claims("bob", "notes:read"));
    assert_eq!(
        send(&router, "GET", "/example/notes", Some(&alice), &[])
            .await
            .0,
        200
    );
    assert_eq!(
        send(&router, "GET", "/example/notes", Some(&alice), &[])
            .await
            .0,
        429
    );
    assert_eq!(
        send(&router, "GET", "/example/notes", Some(&bob), &[])
            .await
            .0,
        200
    );
    config.http.auth = None;
    config.http.writes_per_minute = 1;
    let router = app(AppState::new(fixture.db.clone()), &config);
    assert_eq!(
        send(&router, "POST", "/example/notes", None, &[]).await.0,
        415
    );
    assert_eq!(
        send(&router, "POST", "/example/notes", None, &[]).await.0,
        429
    );
    assert_eq!(
        send(&router, "GET", "/example/notes", None, &[]).await.0,
        200
    );
    fixture.cleanup().await;
}

#[test]
fn auth_configuration_rejects_missing_material_and_unsafe_origins() {
    for entries in [
        vec![("AUTH_MODE", "bearer")],
        vec![("AUTH_MODE", "unknown")],
        vec![("AUTH_PUBLIC_KEY_PEM", "secret-sentinel")],
        vec![("CORS_ORIGINS", "*")],
        vec![("CORS_ORIGINS", "https://client.example/path")],
        vec![("RATE_MAX_KEYS", "0")],
    ] {
        let result = Config::from_lookup(|key| {
            if key == "DATABASE_URL" {
                Some("postgres://local/db".into())
            } else {
                entries
                    .iter()
                    .find(|(k, _)| *k == key)
                    .map(|(_, v)| v.to_string())
            }
        });
        assert!(result.is_err());
        assert!(!result.err().unwrap().contains("secret-sentinel"));
    }
}
