use axum::{
    Json, Router,
    body::Body,
    http::Request,
    routing::{get, post},
};
use bracel::{
    Application,
    config::Config,
    http::middleware::{Access, Middleware, Policies},
};
use std::time::Duration;
use tower::ServiceExt;

fn request(path: &str) -> Request<Body> {
    Request::builder()
        .uri(path)
        .header("origin", "https://client.example")
        .body(Body::empty())
        .unwrap()
}

fn router(config: Config) -> Router {
    let policies = Policies::new(&config);
    let routes = Router::new()
        .route("/ok", get(|| async { "ok" }))
        .route(
            "/slow",
            get(|| async {
                tokio::time::sleep(Duration::from_millis(30)).await;
                "ok"
            }),
        )
        .route(
            "/body",
            post(|Json(value): Json<serde_json::Value>| async move { Json(value) }),
        );
    Application::new(config)
        .merge(policies.apply(routes, Access::Public))
        .merge(policies.apply(
            Router::new().route("/private", get(|| async { "secret" })),
            Access::Scope("read"),
        ))
        .build(())
}

#[tokio::test]
async fn removable_defaults_preserve_explicit_authentication() {
    let mut config = Config::from_lookup(|_| None).unwrap();
    config.cors_origins = vec!["https://client.example".parse().unwrap()];
    config.request_timeout = Duration::from_millis(5);
    config.body_limit = 8;
    let app = router(config.clone());
    let response = app.clone().oneshot(request("/ok")).await.unwrap();
    assert!(response.headers().contains_key("x-request-id"));
    assert_eq!(
        response.headers()["access-control-allow-origin"],
        "https://client.example"
    );
    assert_eq!(
        app.clone()
            .oneshot(request("/slow"))
            .await
            .unwrap()
            .status(),
        408
    );
    let body = || {
        Request::builder()
            .method("POST")
            .uri("/body")
            .header("content-type", "application/json")
            .body(Body::from("{\"value\":\"long body\"}"))
            .unwrap()
    };
    assert_eq!(app.oneshot(body()).await.unwrap().status(), 413);

    config.middleware.clear();
    let app = router(config);
    let response = app.clone().oneshot(request("/ok")).await.unwrap();
    assert!(!response.headers().contains_key("x-request-id"));
    assert!(
        !response
            .headers()
            .contains_key("access-control-allow-origin")
    );
    assert_eq!(
        app.clone()
            .oneshot(request("/slow"))
            .await
            .unwrap()
            .status(),
        200
    );
    assert_eq!(app.clone().oneshot(body()).await.unwrap().status(), 200);
    assert_eq!(
        app.oneshot(request("/private")).await.unwrap().status(),
        503
    );
}

#[tokio::test]
async fn rate_limit_can_be_removed_without_removing_other_layers() {
    let mut config = Config::from_lookup(|_| None).unwrap();
    config.anonymous_per_minute = 1;
    let app = router(config.clone());
    assert_eq!(
        app.clone().oneshot(request("/ok")).await.unwrap().status(),
        200
    );
    assert_eq!(app.oneshot(request("/ok")).await.unwrap().status(), 429);
    config.middleware.retain(|m| *m != Middleware::RateLimit);
    let app = router(config);
    for _ in 0..3 {
        let response = app.clone().oneshot(request("/ok")).await.unwrap();
        assert_eq!(response.status(), 200);
        assert!(response.headers().contains_key("x-request-id"));
    }
}
