use bracel::{
    Application,
    axum::{
        Json, Router,
        body::{Body, to_bytes},
        extract::State,
        http::Request,
        routing::get,
    },
    config::Config,
    http::{
        middleware::{Access, Policies},
        response::Data,
    },
};
use tower::ServiceExt;

#[derive(Clone)]
struct Inventory {
    label: String,
}

#[tokio::test]
async fn application_owns_state_and_routes_without_a_database() {
    let config = Config::from_lookup(|_| None).unwrap();
    let policies = Policies::new(&config);
    let routes = policies.apply(
        Router::new().route(
            "/inventory",
            get(|State(state): State<Inventory>| async move { Json(Data::new(state.label)) }),
        ),
        Access::Public,
    );
    let app = Application::new(config).merge(routes).build(Inventory {
        label: "stock".into(),
    });
    let ok = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/inventory")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ok.status(), 200);
    assert!(ok.headers().contains_key("x-request-id"));
    let body = to_bytes(ok.into_body(), 4096).await.unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&body).unwrap()["data"],
        "stock"
    );
    for (method, path, status) in [("POST", "/inventory", 405), ("GET", "/example/notes", 404)] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), status);
        assert_eq!(
            response.headers()["content-type"],
            "application/problem+json"
        );
    }
}
