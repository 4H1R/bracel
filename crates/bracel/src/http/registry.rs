use crate::{
    config::Config,
    http::middleware::{Access, Policies},
};
use axum::Router;
use serde_json::{Value, json};
use utoipa::openapi::OpenApi;
use utoipa_axum::router::{OpenApiRouter, UtoipaMethodRouter};

#[derive(Clone, Copy)]
pub enum RoutePolicy {
    Exempt,
    Public,
    Scope(&'static str),
    /// Only for an explicitly enabled teaching route, public when auth is disabled.
    Example(&'static str),
}

/// Registration is the common source of runtime routes, contracts and inspection.
pub struct Registry<S> {
    router: Router<S>,
    api: OpenApi,
    policies: Policies,
    bearer: bool,
}

impl<S: Clone + Send + Sync + 'static> Registry<S> {
    pub fn new(config: &Config, api: OpenApi) -> Self {
        use utoipa::OpenApi as _;
        #[derive(utoipa::OpenApi)]
        #[openapi(components(schemas(crate::http::error::Problem)))]
        struct Errors;
        let mut api = api;
        api.merge(Errors::openapi());
        Self {
            router: Router::new(),
            api,
            policies: Policies::new(config),
            bearer: config.auth.is_some(),
        }
    }

    pub fn register(
        &mut self,
        routes: UtoipaMethodRouter<S>,
        policy: RoutePolicy,
        enabled: bool,
        parameters: Vec<Value>,
    ) {
        let (router, api) = OpenApiRouter::default().routes(routes).split_for_parts();
        self.register_parts(router, api, policy, enabled, parameters);
    }

    /// Register a runtime-generated schema and its handler through the same policy pipeline.
    pub fn register_operation(
        &mut self,
        path: &str,
        method: &str,
        router: axum::routing::MethodRouter<S>,
        mut operation: Value,
        policy: RoutePolicy,
        enabled: bool,
    ) -> Result<(), serde_json::Error> {
        if let Some(parameters) = operation
            .get_mut("parameters")
            .and_then(Value::as_array_mut)
        {
            for parameter in parameters {
                if parameter.get("$ref").is_none() && parameter.get("required").is_none() {
                    parameter["required"] = json!(parameter["in"] == "path");
                }
            }
        }
        let mut document =
            json!({"openapi":"3.1.0","info":{"title":"Operations","version":"1"},"paths":{}});
        document["paths"][path] = json!({});
        document["paths"][path][method] = operation;
        let api = serde_json::from_value(document)?;
        self.register_parts(
            Router::new().route(path, router),
            api,
            policy,
            enabled,
            vec![],
        );
        Ok(())
    }

    fn register_parts(
        &mut self,
        mut router: Router<S>,
        api: OpenApi,
        policy: RoutePolicy,
        enabled: bool,
        parameters: Vec<Value>,
    ) {
        let scope = match policy {
            RoutePolicy::Scope(scope) => Some(scope),
            RoutePolicy::Example(scope) if self.bearer => Some(scope),
            _ => None,
        };
        let exempt = matches!(policy, RoutePolicy::Exempt);
        if !exempt {
            router = self
                .policies
                .apply(router, scope.map(Access::Scope).unwrap_or(Access::Public));
        }
        if enabled {
            self.router = self.router.clone().merge(router);
        }
        let mut value = serde_json::to_value(api).expect("serializable contract");
        value["components"]["securitySchemes"]["bearerAuth"] = json!({"type":"http","scheme":"bearer","description":"RS256 JWT access token, or an explicitly configured opaque machine token"});
        for item in value["paths"].as_object_mut().expect("paths").values_mut() {
            for (method, operation) in item.as_object_mut().expect("path") {
                if ![
                    "get", "post", "put", "patch", "delete", "head", "options", "trace",
                ]
                .contains(&method.as_str())
                {
                    continue;
                }
                operation["x-enabled"] = json!(enabled);
                operation["responses"]["408"] = problem_response("Request deadline exceeded");
                operation["x-authentication"] =
                    json!(if scope.is_some() { "bearer" } else { "public" });
                operation["x-required-scope"] = json!(scope);
                operation["x-rate-policy"] = json!(if exempt {
                    "exempt"
                } else if matches!(method.as_str(), "get" | "head" | "options") {
                    "api"
                } else {
                    "writes"
                });
                if scope.is_some() {
                    operation["security"] = json!([{"bearerAuth":[]}]);
                }
                if matches!(policy, RoutePolicy::Example(_)) {
                    operation["security"] = json!([{}, {"bearerAuth":[]}]);
                }
                if !parameters.is_empty() {
                    if operation.get("parameters").is_none() {
                        operation["parameters"] = json!([]);
                    }
                    operation["parameters"]
                        .as_array_mut()
                        .expect("parameters")
                        .extend(parameters.clone());
                }
                if !exempt {
                    for (status, description) in [
                        ("429", "Request rate exceeded"),
                        ("503", "Request capacity or dependency unavailable"),
                    ] {
                        operation["responses"][status] = problem_response(description);
                    }
                    operation["responses"]["429"]["headers"] =
                        json!({"Retry-After":{"schema":{"type":"string"}}});
                }
                if scope.is_some() || matches!(policy, RoutePolicy::Example(_)) {
                    operation["responses"]["401"] =
                        problem_response("A valid access token is required");
                    operation["responses"]["401"]["headers"] =
                        json!({"WWW-Authenticate":{"schema":{"type":"string"}}});
                    operation["responses"]["403"] =
                        problem_response("Required permission is missing");
                }
            }
        }
        self.api
            .merge(serde_json::from_value(value).expect("valid generated contract"));
    }

    pub fn openapi(&self) -> OpenApi {
        self.api.clone()
    }
    pub fn request_schema(
        &mut self,
        path: &str,
        method: &str,
        schema: Value,
    ) -> Result<(), serde_json::Error> {
        let mut value = serde_json::to_value(&self.api)?;
        value["paths"][path][method]["requestBody"] =
            json!({"required":true,"content":{"application/json":{"schema":schema}}});
        self.api = serde_json::from_value(value)?;
        Ok(())
    }
    pub fn into_router(self) -> Router<S> {
        self.router
    }
    pub fn inventory(&self) -> Vec<Value> {
        let value = serde_json::to_value(&self.api).expect("serializable contract");
        let mut routes = Vec::new();
        for (path, item) in value["paths"].as_object().expect("paths") {
            for method in [
                "get", "post", "put", "patch", "delete", "head", "options", "trace",
            ] {
                if let Some(op) = item.get(method) {
                    routes.push(json!({"method":method.to_ascii_uppercase(), "path":path,
                        "enabled":op["x-enabled"], "authentication":op["x-authentication"],
                        "required_scope":op["x-required-scope"], "rate_policy":op["x-rate-policy"],
                        "query_parameters":op["parameters"].as_array().map(|p| p.iter().filter(|p| p["in"] == "query").collect::<Vec<_>>()).unwrap_or_default()}));
                }
            }
        }
        routes
    }
}

fn problem_response(description: &str) -> Value {
    json!({"description":description,"content":{"application/problem+json":{"schema":{"$ref":"#/components/schemas/Problem"}}}})
}
