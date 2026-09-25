//! Shared declarations drive the static spec and the runtime inspect inventory.
use crate::features::notes;
use serde_json::json;

pub(super) struct Contracts;
impl utoipa::Modify for Contracts {
    fn modify(&self, api: &mut utoipa::openapi::OpenApi) {
        let mut value = serde_json::to_value(&*api).expect("OpenAPI serialization");
        value["components"]["securitySchemes"]["bearerAuth"] = json!({"type":"http","scheme":"bearer","bearerFormat":"JWT","description":"AUTH_MODE=bearer: RS256 access token with typ=at+jwt, configured issuer/audience and required route scope. AUTH_MODE=off is only for local teaching."});
        value["paths"]["/example/notes"]["get"]["parameters"]
            .as_array_mut()
            .expect("pagination parameters")
            .extend(notes::query::parameters());
        for (path, item) in value["paths"].as_object_mut().expect("paths") {
            if !path.starts_with("/example/") {
                continue;
            }
            for (method, operation) in item.as_object_mut().expect("path") {
                if !matches!(method.as_str(), "get" | "post") {
                    continue;
                }
                operation["security"] = json!([{}, {"bearerAuth":[]}]);
                operation["x-required-scope"] = json!(if method == "get" {
                    notes::http::READ_SCOPE
                } else {
                    notes::http::WRITE_SCOPE
                });
                operation["x-rate-policy"] = json!(if method == "get" { "api" } else { "writes" });
                for (status, description) in [
                    ("401", "Missing or invalid access token"),
                    ("403", "Missing required scope"),
                    ("429", "Rate exceeded; retry after the indicated seconds"),
                    (
                        "503",
                        "Database, authentication or request capacity unavailable",
                    ),
                ] {
                    operation["responses"][status] = json!({"description":description,"content":{"application/problem+json":{"schema":{"$ref":"#/components/schemas/Problem"}}}});
                }
                operation["responses"]["401"]["headers"] =
                    json!({"WWW-Authenticate":{"schema":{"type":"string"}}});
                operation["responses"]["429"]["headers"] =
                    json!({"Retry-After":{"schema":{"type":"string"}}});
            }
        }
        *api = serde_json::from_value(value).expect("valid generated contracts");
    }
}
