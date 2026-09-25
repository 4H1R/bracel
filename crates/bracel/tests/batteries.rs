#![cfg(feature = "testing")]
use bracel::utoipa;
use bracel::{
    axum::{Json, extract::State},
    config::Config,
    http::{
        error::AppError,
        extract::{Validate, ValidatedJson},
        fields::Fields,
        registry::{Registry, RoutePolicy},
        response::Data,
    },
    testing::TestClient,
    utoipa_axum::routes,
};
use serde::Deserialize;
use serde_json::json;
#[derive(Clone)]
struct StateValue(&'static str);
#[derive(Deserialize)]
struct Input(Fields);
impl Validate for Input {
    type Output = String;
    fn validate(self) -> Result<String, AppError> {
        let mut fields = self.0;
        let title = fields.text("title", 10);
        fields.finish()?;
        Ok(title)
    }
}
#[utoipa::path(post,path="/items",responses((status=200,body=Data<String>)))]
async fn create(
    State(value): State<StateValue>,
    ValidatedJson(input): ValidatedJson<Input>,
) -> Json<Data<String>> {
    Json(Data::new(format!("{}:{input}", value.0)))
}
#[tokio::test]
async fn registry_is_the_route_contract_and_input_validation_source() {
    let config = Config::from_lookup(|_| None).unwrap();
    let mut registry = Registry::new(&config, utoipa::openapi::OpenApi::default());
    registry.register(routes!(create), RoutePolicy::Public, true, vec![]);
    assert_eq!(registry.inventory()[0]["path"], "/items");
    assert_eq!(registry.inventory()[0]["rate_policy"], "writes");
    assert!(registry.openapi().paths.paths.contains_key("/items"));
    let client = TestClient::new(
        bracel::Application::new(config.clone())
            .merge(registry.into_router())
            .build(StateValue("test")),
    );
    assert_eq!(
        client
            .request("POST", "/items", Some(json!({"title":" hi "})))
            .await
            .body["data"],
        "test:hi"
    );
    client
        .request(
            "POST",
            "/items",
            Some(json!({"title":"","secret":"not echoed"})),
        )
        .await
        .assert_issue("title", "too_small");
    let mut disabled = Registry::new(&config, utoipa::openapi::OpenApi::default());
    disabled.register(routes!(create), RoutePolicy::Scope("write"), false, vec![]);
    assert_eq!(disabled.inventory()[0]["enabled"], false);
    let client = TestClient::new(
        bracel::Application::new(config)
            .merge(disabled.into_router())
            .build(StateValue("test")),
    );
    client
        .request("POST", "/items", Some(json!({"title":"hi"})))
        .await
        .assert_status(404);
}
