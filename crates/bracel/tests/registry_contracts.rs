use bracel::{
    axum::routing::get,
    config::Config,
    http::{
        middleware::Middleware,
        registry::{Registry, RoutePolicy},
    },
};
use serde_json::json;

#[tokio::test]
async fn registries_share_concurrency_when_selecting_a_different_verifier() {
    use bracel::{
        Application,
        axum::{body::Body, http::Request},
        http::middleware::Policies,
    };
    use tower::ServiceExt;
    let mut config = Config::from_lookup(|_| None).unwrap();
    config.max_in_flight = 1;
    let shared = Policies::new(&config);
    let mut first = Registry::<()>::with_policies(&config, Default::default(), shared.clone());
    let entered = std::sync::Arc::new(tokio::sync::Notify::new());
    let release = std::sync::Arc::new(tokio::sync::Notify::new());
    let (notify, wait) = (entered.clone(), release.clone());
    first
        .register_operation(
            "/first",
            "get",
            get(move || {
                let (notify, wait) = (notify.clone(), wait.clone());
                async move {
                    notify.notify_one();
                    wait.notified().await;
                    "ok"
                }
            }),
            json!({"responses":{"200":{"description":"OK"}}}),
            RoutePolicy::Public,
            true,
        )
        .unwrap();
    config.auth = Some(bracel::identity::BearerAuth::pending("another-issuer", "api").unwrap());
    let mut second = Registry::<()>::with_policies(&config, Default::default(), shared);
    second
        .register_operation(
            "/second",
            "get",
            get(|| async { "ok" }),
            json!({"responses":{"200":{"description":"OK"}}}),
            RoutePolicy::Public,
            true,
        )
        .unwrap();
    let router = Application::new(config)
        .merge(first.into_router())
        .merge(second.into_router())
        .build(());
    let request = |path| Request::builder().uri(path).body(Body::empty()).unwrap();
    let running = tokio::spawn(router.clone().oneshot(request("/first")));
    entered.notified().await;
    assert_eq!(
        router
            .clone()
            .oneshot(request("/second"))
            .await
            .unwrap()
            .status(),
        503
    );
    release.notify_one();
    assert_eq!(running.await.unwrap().unwrap().status(), 200);
    assert_eq!(
        router.oneshot(request("/second")).await.unwrap().status(),
        200
    );
}

#[test]
fn disabled_rate_limit_has_no_synthetic_429_response() {
    let mut config = Config::from_lookup(|_| None).unwrap();
    config.middleware.retain(|m| *m != Middleware::RateLimit);
    let mut registry = Registry::<()>::new(&config, Default::default());
    registry
        .register_operation(
            "/probe",
            "get",
            get(|| async { "ok" }),
            json!({"responses":{"200":{"description":"OK"}}}),
            RoutePolicy::Public,
            true,
        )
        .unwrap();
    let api = serde_json::to_value(registry.openapi()).unwrap();
    assert!(
        api["paths"]["/probe"]["get"]["responses"]
            .get("429")
            .is_none()
    );
}
