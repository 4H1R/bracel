//! Runnable API-only reference workflow for optional framework packages.
mod projects;
use projects::{
    create, import_projects, incoming_webhook, missing, notification_read, notifications, read,
    representation, tenant_create, tenant_projects, update,
};
mod realtime;
use realtime::{RealtimeState, events_socket, events_sse};
mod files;
use files::{file_complete, file_delete, file_download, file_init, file_metadata, file_upload};
mod collections;
pub(crate) mod commands;
use axum::{
    Extension, Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
};
use bracel::http::extract::UniqueJson;
use bracel::{
    http::{
        error::AppError,
        registry::{Registry, RoutePolicy},
        schema::{Field, Rule, Schema},
    },
    identity::Principal,
};
use bracel_data::{Attempt, Mutation, sql};
use sea_orm::{ConnectionTrait, DatabaseConnection, QueryResult, TransactionTrait};
use serde_json::{Value, json};
use uuid::Uuid;

pub fn schema() -> Schema {
    Schema(vec![
        Field::required("name", Rule::Text { min: 1, max: 200 }),
        Field::required("active", Rule::Boolean).optional(),
        Field::required(
            "budget",
            Rule::Decimal {
                precision: 12,
                scale: 2,
            },
        )
        .optional(),
        Field::required("description", Rule::Text { min: 0, max: 2000 })
            .optional()
            .nullable(),
        Field::required(
            "labels",
            Rule::Array {
                item: Box::new(Rule::Text { min: 1, max: 40 }),
                max: 20,
            },
        )
        .optional(),
        Field::required("due_at", Rule::Timestamp)
            .optional()
            .nullable(),
        Field::required(
            "status",
            Rule::Enum {
                values: vec!["planned".into(), "active".into(), "done".into()],
            },
        )
        .optional(),
    ])
}

pub fn registry(
    config: &bracel::config::Config,
    enabled: bool,
    files: bool,
) -> Registry<DatabaseConnection> {
    registry_with_policies(
        config,
        enabled,
        files,
        bracel::http::middleware::Policies::new(config),
    )
}
pub(crate) fn registry_with_policies(
    config: &bracel::config::Config,
    enabled: bool,
    files: bool,
    policies: bracel::http::middleware::Policies,
) -> Registry<DatabaseConnection> {
    let mut registry = Registry::with_policies(
        config,
        utoipa::openapi::OpenApi::new(
            utoipa::openapi::Info::new("Optional API packages", "1"),
            utoipa::openapi::Paths::new(),
        ),
        policies,
    );
    macro_rules! route {
        ($path:literal,$method:literal,$router:expr,$id:literal,$policy:expr,$enabled:expr,$request:expr) => {{
            let mut operation = json!({"operationId":$id,"responses":{"200":{"description":"Success","content":{"application/json":{"schema":{"type":"object"}}}}},"parameters":[]});
            if $path.contains("{id}") { operation["parameters"].as_array_mut().unwrap().push(json!({"name":"id","in":"path","required":true,"schema":{"type":"string","format":"uuid"}})); }
            let request: Option<Value>=$request;
            if let Some(schema)=request { operation["requestBody"]=json!({"required":true,"content":{"application/json":{"schema":schema}}}); }
            for status in ["400","404","409","412","413","422","428"] { operation["responses"][status]=json!({"description":"Request rejected","content":{"application/problem+json":{"schema":{"$ref":"#/components/schemas/Problem"}}}}); }
            if $method=="post" && matches!($path,"/api/projects"|"/api/projects/import"|"/api/tenants/{id}/projects"|"/api/files") { let response=operation["responses"]["200"].take(); operation["responses"].as_object_mut().unwrap().remove("200");operation["responses"]["201"]=response; }
            if $method=="delete" { operation["responses"].as_object_mut().unwrap().remove("200");operation["responses"]["204"]=json!({"description":"Deleted"}); }
            if matches!($path,"/api/projects"|"/api/projects/import"|"/api/tenants/{id}/projects") && $method=="post" { operation["parameters"].as_array_mut().unwrap().push(json!({"name":"Idempotency-Key","in":"header","required":true,"schema":{"type":"string","maxLength":200}})); }
            if $method=="patch" || ($path.starts_with("/api/projects/") && $method=="delete") { operation["parameters"].as_array_mut().unwrap().push(json!({"name":"If-Match","in":"header","required":true,"schema":{"type":"string"},"description":"Quoted positive resource version"})); }
            if matches!($path,"/api/events"|"/api/socket") {
                operation["parameters"]=json!([{"name":"topic","in":"query","required":true,"schema":{"type":"string","enum":["projects","notifications"]}},{"name":"Last-Event-ID","in":"header","schema":{"type":"string"}}]);
                operation["responses"]["200"]=json!({"description":"Authorized event stream; reconnect using Last-Event-ID, recover retention gaps with snapshot","content":{"text/event-stream":{"schema":{"type":"string"}}}});
                if $path=="/api/socket" { operation["responses"].as_object_mut().unwrap().remove("200");operation["responses"]["101"]=json!({"description":"WebSocket upgrade; versioned events and validated JSON messages"}); }
            }
            if $path=="/api/projects" && $method=="get" { for name in ["filter[active]","filter[status]","q","fields","include","after","before","offset","limit"] { operation["parameters"].as_array_mut().unwrap().push(json!({"name":name,"in":"query","schema":{"type":"string"}})); } }
            if $path=="/api/files/{id}/content" {
                if $method=="get" {operation["responses"]["200"]=json!({"description":"Authorized file download","content":{"application/octet-stream":{"schema":{"type":"string","format":"binary"}}}});}
                if $method=="put" {operation["requestBody"]=json!({"required":true,"content":{"application/octet-stream":{"schema":{"type":"string","format":"binary"}}}});}
            }
            if $path=="/api/webhooks/incoming" {for name in ["webhook-id","webhook-timestamp","webhook-signature"] {operation["parameters"].as_array_mut().unwrap().push(json!({"name":name,"in":"header","required":true,"schema":{"type":"string"}}));}}
            registry.register_operation($path,$method,$router,operation,$policy,$enabled).expect("valid operation schema");
        }}
    }
    route!(
        "/api/operations/metrics",
        "get",
        get(metrics),
        "operations_metrics",
        RoutePolicy::Scope("ops:read"),
        enabled,
        None
    );
    route!(
        "/api/webhooks/incoming",
        "post",
        post(incoming_webhook),
        "incoming_webhook",
        RoutePolicy::Public,
        enabled,
        Some(json!({"type":"object"}))
    );
    route!(
        "/api/projects",
        "get",
        get(collections::list),
        "projects_list",
        RoutePolicy::Scope("projects:read"),
        enabled,
        None
    );
    route!(
        "/api/projects/{id}",
        "get",
        get(read),
        "projects_read",
        RoutePolicy::Scope("projects:read"),
        enabled,
        None
    );
    route!(
        "/api/projects/snapshot",
        "get",
        get(collections::snapshot),
        "projects_snapshot",
        RoutePolicy::Scope("projects:read"),
        enabled,
        None
    );
    route!(
        "/api/projects",
        "post",
        post(create),
        "projects_create",
        RoutePolicy::Scope("projects:write"),
        enabled,
        Some(schema().openapi(false))
    );
    route!(
        "/api/projects/{id}",
        "patch",
        axum::routing::patch(update),
        "projects_patch",
        RoutePolicy::Scope("projects:write"),
        enabled,
        Some(schema().openapi(true))
    );
    route!(
        "/api/projects/{id}",
        "delete",
        axum::routing::delete(collections::delete),
        "projects_delete",
        RoutePolicy::Scope("projects:write"),
        enabled,
        None
    );
    route!(
        "/api/projects/{id}/restore",
        "post",
        post(collections::restore),
        "projects_restore",
        RoutePolicy::Scope("projects:write"),
        enabled,
        None
    );
    route!(
        "/api/events",
        "get",
        get(events_sse),
        "events_subscribe",
        RoutePolicy::Scope("events:read"),
        enabled,
        None
    );
    route!(
        "/api/socket",
        "get",
        get(events_socket),
        "socket_subscribe",
        RoutePolicy::Scope("events:read"),
        enabled,
        None
    );
    route!(
        "/api/notifications",
        "get",
        get(notifications),
        "notifications_list",
        RoutePolicy::Scope("notifications:read"),
        enabled,
        None
    );
    route!(
        "/api/notifications/{id}/read",
        "post",
        post(notification_read),
        "notifications_read",
        RoutePolicy::Scope("notifications:write"),
        enabled,
        None
    );
    route!(
        "/api/files/{id}",
        "get",
        get(file_metadata),
        "files_metadata",
        RoutePolicy::Scope("files:read"),
        enabled && files,
        None
    );
    route!(
        "/api/files/{id}/content",
        "get",
        get(file_download),
        "files_download",
        RoutePolicy::Scope("files:read"),
        enabled && files,
        None
    );
    route!(
        "/api/files",
        "post",
        post(file_init),
        "files_initialize",
        RoutePolicy::Scope("files:write"),
        enabled && files,
        Some(
            json!({"type":"object","additionalProperties":false,"required":["name","size","sha256","content_type"],"properties":{"name":{"type":"string","maxLength":200},"size":{"type":"integer","minimum":0,"maximum":1048576},"sha256":{"type":"string","pattern":"^[0-9a-fA-F]{64}$"},"content_type":{"type":"string","enum":["text/plain","application/octet-stream","application/pdf","image/png","image/jpeg"]}}})
        )
    );
    route!(
        "/api/files/{id}",
        "delete",
        axum::routing::delete(file_delete),
        "files_delete",
        RoutePolicy::Scope("files:write"),
        enabled && files,
        None
    );
    route!(
        "/api/files/{id}/content",
        "put",
        axum::routing::put(file_upload).layer(axum::extract::DefaultBodyLimit::max(1024 * 1024)),
        "files_upload",
        RoutePolicy::Scope("files:write"),
        enabled && files,
        None
    );
    route!(
        "/api/files/{id}/complete",
        "post",
        post(file_complete),
        "files_complete",
        RoutePolicy::Scope("files:write"),
        enabled && files,
        None
    );
    route!(
        "/api/projects/import",
        "post",
        post(import_projects),
        "projects_import",
        RoutePolicy::Scope("projects:write"),
        enabled,
        Some(json!({"type":"array","items":schema().openapi(false),"minItems":1,"maxItems":50}))
    );
    route!(
        "/api/tenants/{id}/projects",
        "get",
        get(tenant_projects),
        "tenant_projects",
        RoutePolicy::Scope("projects:read"),
        enabled,
        None
    );
    route!(
        "/api/tenants/{id}/projects",
        "post",
        post(tenant_create),
        "tenant_projects_create",
        RoutePolicy::Scope("projects:write"),
        enabled,
        Some(schema().openapi(false))
    );
    registry
}

pub(crate) fn decorate(
    mut router: Router<DatabaseConnection>,
    db: DatabaseConnection,
    config: &bracel::config::Config,
    providers: crate::provider_settings::Providers,
    resources: &crate::bootstrap::Resources,
) -> Router<crate::AppState> {
    if let (Some(auth), Some(events)) = (config.auth.clone(), resources.events.clone()) {
        router = router.layer(Extension(RealtimeState {
            events,
            auth,
            origins: config.cors_origins.clone(),
        }));
    }
    if let Some(files) = &resources.files {
        router = router.layer(Extension(files.clone()));
    }
    router.layer(Extension(providers)).with_state(db)
}
async fn metrics(
    Extension(metrics): Extension<bracel::http::metrics::Metrics>,
    State(db): State<DatabaseConnection>,
) -> Result<Json<Value>, AppError> {
    let row=db.query_one_raw(sql("SELECT count(*) FILTER(WHERE status='pending') AS pending,count(*) FILTER(WHERE status='failed') AS failed FROM bracel_jobs",vec![])).await?.ok_or_else(bracel_data::conflict)?;
    Ok(Json(
        json!({"data":{"http":metrics.snapshot(),"jobs":{"pending":row.try_get::<i64>("","pending")?,"failed":row.try_get::<i64>("","failed")?}}}),
    ))
}
