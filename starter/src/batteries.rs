//! Runnable API-only reference workflow for optional framework packages.
#[path = "batteries_collections.rs"]
mod collections;
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
    let mut registry = Registry::new(
        config,
        utoipa::openapi::OpenApi::new(
            utoipa::openapi::Info::new("Optional API packages", "1"),
            utoipa::openapi::Paths::new(),
        ),
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

pub fn router(db: DatabaseConnection, config: &bracel::config::Config) -> Router<crate::AppState> {
    let root = std::env::var_os("FILES_ROOT");
    let mut router = registry(config, true, root.is_some()).into_router();
    if let Some(auth) = config.auth.clone() {
        let events = bracel_realtime::EventStore::new(db.clone(), 100)
            .unwrap_or_else(|_| panic!("valid connection limit"));
        let stopping = events.clone();
        tokio::spawn(async move {
            crate::commands::shutdown().await;
            stopping.shutdown();
        });
        router = router.layer(Extension(RealtimeState {
            events,
            auth,
            origins: config.cors_origins.clone(),
        }));
    }
    if let Some(root) = root {
        std::fs::create_dir_all(&root).expect("create configured files directory");
        let storage = bracel_integrations::storage::Storage::local(root, 1024 * 1024)
            .expect("valid local storage");
        router = router.layer(Extension(
            bracel_files::Files::new(db.clone(), storage, 1024 * 1024)
                .unwrap_or_else(|_| panic!("valid file limit")),
        ));
    }
    router.with_state(db)
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

async fn file_init(
    Extension(files): Extension<bracel_files::Files>,
    Extension(principal): Extension<Principal>,
    Json(input): Json<bracel_files::Upload>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    Ok((
        StatusCode::CREATED,
        Json(json!({"data":files.initialize(&principal.cursor_scope(),input).await?})),
    ))
}
async fn file_metadata(
    Extension(files): Extension<bracel_files::Files>,
    Extension(principal): Extension<Principal>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    Ok(Json(
        json!({"data":files.metadata(&principal.cursor_scope(),id).await?}),
    ))
}
async fn file_upload(
    Extension(files): Extension<bracel_files::Files>,
    Extension(principal): Extension<Principal>,
    Path(id): Path<Uuid>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, AppError> {
    Ok(Json(
        json!({"data":files.upload(&principal.cursor_scope(),id,body.to_vec()).await?}),
    ))
}
async fn file_complete(
    Extension(files): Extension<bracel_files::Files>,
    Extension(principal): Extension<Principal>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    Ok(Json(
        json!({"data":files.complete(&principal.cursor_scope(),id).await?}),
    ))
}
async fn file_delete(
    Extension(files): Extension<bracel_files::Files>,
    Extension(principal): Extension<Principal>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    files.delete(&principal.cursor_scope(), id).await?;
    Ok(StatusCode::NO_CONTENT)
}
async fn file_download(
    Extension(files): Extension<bracel_files::Files>,
    Extension(principal): Extension<Principal>,
    Path(id): Path<Uuid>,
) -> Result<axum::response::Response, AppError> {
    use axum::response::IntoResponse;
    let (file, bytes) = files.download(&principal.cursor_scope(), id).await?;
    Ok((
        [
            ("content-type", file.content_type),
            (
                "content-disposition",
                format!("attachment; filename=\"{id}\""),
            ),
            ("x-content-type-options", "nosniff".into()),
        ],
        bytes,
    )
        .into_response())
}

#[derive(Clone)]
struct RealtimeState {
    events: bracel_realtime::EventStore,
    auth: bracel::identity::BearerAuth,
    origins: Vec<axum::http::HeaderValue>,
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct SubscriptionQuery {
    topic: String,
}
async fn subscription(
    state: RealtimeState,
    principal: Principal,
    query: SubscriptionQuery,
    headers: &HeaderMap,
) -> Result<bracel_realtime::Subscription, AppError> {
    if !matches!(query.topic.as_str(), "projects" | "notifications") {
        return Err(AppError::new(
            StatusCode::FORBIDDEN,
            "Topic is not available",
        ));
    }
    let token = headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .ok_or_else(|| AppError::new(StatusCode::UNAUTHORIZED, "Bearer token is required"))?;
    state
        .events
        .subscribe(
            principal.cursor_scope(),
            query.topic,
            headers.get("last-event-id").and_then(|h| h.to_str().ok()),
            state.auth,
            token.into(),
        )
        .await
}
async fn events_sse(
    Extension(state): Extension<RealtimeState>,
    Extension(principal): Extension<Principal>,
    axum::extract::Query(query): axum::extract::Query<SubscriptionQuery>,
    headers: HeaderMap,
) -> Result<axum::response::Response, AppError> {
    use axum::response::IntoResponse;
    Ok(subscription(state, principal, query, &headers)
        .await?
        .sse()
        .into_response())
}
async fn events_socket(
    State(db): State<DatabaseConnection>,
    Extension(state): Extension<RealtimeState>,
    Extension(principal): Extension<Principal>,
    axum::extract::Query(query): axum::extract::Query<SubscriptionQuery>,
    headers: HeaderMap,
    upgrade: axum::extract::ws::WebSocketUpgrade,
) -> Result<axum::response::Response, AppError> {
    if headers
        .get("origin")
        .is_some_and(|origin| !state.origins.contains(origin))
    {
        return Err(AppError::new(
            StatusCode::FORBIDDEN,
            "Origin is not allowed",
        ));
    }
    let subscription = subscription(state, principal, query, &headers).await?;
    Ok(upgrade
        .max_message_size(8192)
        .max_frame_size(8192)
        .write_buffer_size(1024)
        .max_write_buffer_size(131072)
        .on_upgrade(move |socket| {
            subscription.websocket_with(socket, move |principal, message| {
                let db = db.clone();
                async move {
                    #[derive(serde::Deserialize)]
                    #[serde(deny_unknown_fields)]
                    struct PatchMessage {
                        r#type: String,
                        id: Uuid,
                        version: i64,
                        patch: Value,
                    }
                    let message: PatchMessage =
                        serde_json::from_value(message).map_err(|_| bracel_data::conflict())?;
                    if message.r#type != "projects.patch" || !principal.allows("projects:write") {
                        return Err(AppError::new(
                            StatusCode::FORBIDDEN,
                            "Message permission required",
                        ));
                    }
                    let mut headers = HeaderMap::new();
                    headers.insert(
                        "if-match",
                        format!("\"{}\"", message.version)
                            .parse()
                            .map_err(|_| bracel_data::conflict())?,
                    );
                    let Json(result) = update(
                        State(db),
                        Extension(principal),
                        Path(message.id),
                        headers,
                        UniqueJson(message.patch),
                    )
                    .await?;
                    Ok(json!({"type":"projects.updated","result":result}))
                }
            })
        }))
}

fn missing() -> AppError {
    AppError::new(StatusCode::NOT_FOUND, "Project not found")
}
fn representation(row: QueryResult) -> Result<Value, AppError> {
    let mut value: Value = serde_json::from_str(&row.try_get::<String>("", "document")?)
        .map_err(|_| bracel_data::conflict())?;
    value["id"] = json!(row.try_get::<Uuid>("", "id")?);
    value["version"] = json!(row.try_get::<i64>("", "version")?);
    Ok(value)
}
async fn read(
    State(db): State<DatabaseConnection>,
    Extension(principal): Extension<Principal>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let row=db.query_one_raw(sql("SELECT id,version,document::text AS document FROM projects WHERE id=$1 AND owner=$2 AND deleted_at IS NULL",vec![id.into(),principal.cursor_scope().into()])).await?.ok_or_else(missing)?;
    Ok(Json(json!({"data":representation(row)?})))
}
async fn create(
    State(db): State<DatabaseConnection>,
    Extension(principal): Extension<Principal>,
    headers: HeaderMap,
    UniqueJson(input): UniqueJson,
) -> Result<(StatusCode, Json<Value>), AppError> {
    let fields = schema().validate(input.clone(), false)?;
    let key = headers
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::new(StatusCode::BAD_REQUEST, "Idempotency-Key is required"))?;
    let scope = principal.cursor_scope();
    let result = match Mutation::begin(&db, &scope, "projects.create", key, &input).await? {
        Attempt::Replay(result) => result,
        Attempt::New(mutation) => {
            let document = insert_project(&mutation.transaction, &scope, &scope, fields).await?;
            mutation.finish(201, json!({"data":document})).await?
        }
    };
    Ok((
        StatusCode::from_u16(result.status).map_err(|_| bracel_data::conflict())?,
        Json(result.body),
    ))
}

async fn incoming_webhook(
    State(db): State<DatabaseConnection>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, AppError> {
    let secret = std::env::var("WEBHOOK_INCOMING_SECRET").map_err(|_| {
        AppError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "Webhook receiver is not configured",
        )
    })?;
    let field = |name| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| {
                AppError::new(
                    StatusCode::UNAUTHORIZED,
                    "Webhook signature headers are required",
                )
            })
    };
    let timestamp = field("webhook-timestamp")?
        .parse()
        .map_err(|_| AppError::new(StatusCode::UNAUTHORIZED, "Invalid webhook timestamp"))?;
    let accepted = bracel_delivery::webhook::receive(
        &db,
        "primary",
        secret.as_bytes(),
        bracel_delivery::webhook::SignedRequest {
            timestamp,
            event_id: field("webhook-id")?,
            body: &body,
            signature: field("webhook-signature")?,
        },
    )
    .await?;
    Ok(Json(json!({"data":{"accepted":accepted}})))
}

async fn notifications(
    State(db): State<DatabaseConnection>,
    Extension(principal): Extension<Principal>,
) -> Result<Json<Value>, AppError> {
    Ok(Json(
        json!({"data":bracel_delivery::inbox(&db,&principal.cursor_scope(),None,50).await?}),
    ))
}
async fn notification_read(
    State(db): State<DatabaseConnection>,
    Extension(principal): Extension<Principal>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    bracel_delivery::mark_read(&db, &principal.cursor_scope(), id).await?;
    Ok(Json(json!({"data":{"read":true}})))
}
async fn update(
    State(db): State<DatabaseConnection>,
    Extension(principal): Extension<Principal>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    UniqueJson(input): UniqueJson,
) -> Result<Json<Value>, AppError> {
    let fields = schema().validate(input, true)?;
    let version =
        bracel_data::expected_version(headers.get("if-match").and_then(|v| v.to_str().ok()))?;
    let tx = db.begin().await?;
    let scope = principal.cursor_scope();
    let row=tx.query_one_raw(sql("SELECT id,version,document::text AS document FROM projects WHERE id=$1 AND owner=$2 AND deleted_at IS NULL FOR UPDATE",vec![id.into(),scope.clone().into()])).await?.ok_or_else(missing)?;
    if row.try_get::<i64>("", "version")? != version {
        return Err(bracel_data::stale());
    }
    let mut document: Value = serde_json::from_str(&row.try_get::<String>("", "document")?)
        .map_err(|_| bracel_data::conflict())?;
    document
        .as_object_mut()
        .ok_or_else(bracel_data::conflict)?
        .extend(fields);
    tx.execute_raw(sql("UPDATE projects SET document=$1::jsonb,version=version+1 WHERE id=$2 AND owner=$3 AND version=$4",vec![document.to_string().into(),id.into(),scope.clone().into(),version.into()])).await?;
    bracel_data::audit(
        &tx,
        &scope,
        &scope,
        "project.updated",
        id,
        json!({"version":version+1}),
    )
    .await?;
    bracel_realtime::publish_value(
        &tx,
        &scope,
        "projects",
        "project.updated",
        1,
        json!({"id":id,"version":version+1}),
    )
    .await?;
    tx.commit().await?;
    document["id"] = json!(id);
    document["version"] = json!(version + 1);
    Ok(Json(json!({"data":document})))
}

async fn insert_project(
    tx: &sea_orm::DatabaseTransaction,
    scope: &str,
    actor: &str,
    fields: serde_json::Map<String, Value>,
) -> Result<Value, AppError> {
    let id = Uuid::now_v7();
    let mut document = json!({"active":true,"budget":"0.00","description":null,"labels":[],"due_at":null,"status":"planned"});
    document.as_object_mut().expect("object").extend(fields);
    tx.execute_raw(sql(
        "INSERT INTO projects(id,owner,document) VALUES($1,$2,$3::jsonb)",
        vec![id.into(), scope.into(), document.to_string().into()],
    ))
    .await?;
    bracel_data::audit(tx, scope, actor, "project.created", id, json!({})).await?;
    bracel_realtime::publish_value(
        tx,
        scope,
        "projects",
        "project.created",
        1,
        json!({"id":id,"version":1}),
    )
    .await?;
    document["id"] = json!(id);
    if std::env::var_os("WEBHOOK_URL").is_some() {
        bracel_delivery::webhook::enqueue(
            tx,
            "primary",
            id,
            json!({"type":"project.created","id":id}),
        )
        .await?;
    }
    if let Ok(to) = std::env::var("NOTIFY_EMAIL") {
        bracel_delivery::enqueue_mail(
            tx,
            bracel_delivery::MailIntent {
                scope,
                dedupe: &format!("project-mail:{id}"),
                from: "notifications@example.test",
                to: &to,
                subject: "Project created",
                text: "Your project was created.",
            },
        )
        .await?;
    }
    bracel_delivery::notify(
        tx,
        scope,
        &format!("project:{id}"),
        json!({"kind":"project.created","project_id":id}),
    )
    .await?;
    document["version"] = json!(1);
    Ok(document)
}

async fn import_projects(
    State(db): State<DatabaseConnection>,
    Extension(principal): Extension<Principal>,
    headers: HeaderMap,
    UniqueJson(input): UniqueJson,
) -> Result<(StatusCode, Json<Value>), AppError> {
    let items = input
        .as_array()
        .filter(|items| !items.is_empty() && items.len() <= 50)
        .ok_or_else(bracel_data::conflict)?;
    let validated = items
        .iter()
        .map(|item| schema().validate(item.clone(), false))
        .collect::<Result<Vec<_>, _>>()?;
    let key = headers
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(bracel_data::conflict)?;
    let scope = principal.cursor_scope();
    let result = match Mutation::begin(&db, &scope, "projects.import", key, &input).await? {
        Attempt::Replay(result) => result,
        Attempt::New(mutation) => {
            let mut documents = Vec::new();
            for fields in validated {
                documents
                    .push(insert_project(&mutation.transaction, &scope, &scope, fields).await?);
            }
            mutation.finish(201, json!({"data":documents})).await?
        }
    };
    Ok((StatusCode::CREATED, Json(result.body)))
}

async fn tenant_projects(
    State(db): State<DatabaseConnection>,
    Extension(principal): Extension<Principal>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let tenant = bracel_data::TenantScope::resolve(&db, id, &principal).await?;
    let rows=db.query_all_raw(sql("SELECT id,version,document::text AS document FROM projects WHERE owner=$1 AND deleted_at IS NULL ORDER BY id LIMIT 101",vec![tenant.key().into()])).await?;
    if rows.len() > 100 {
        return Err(AppError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "Tenant snapshot exceeds 100 resources",
        ));
    }
    Ok(Json(
        json!({"data":rows.into_iter().map(representation).collect::<Result<Vec<_>,_>>()?}),
    ))
}
async fn tenant_create(
    State(db): State<DatabaseConnection>,
    Extension(principal): Extension<Principal>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    UniqueJson(input): UniqueJson,
) -> Result<(StatusCode, Json<Value>), AppError> {
    let tenant = bracel_data::TenantScope::resolve(&db, id, &principal).await?;
    tenant.require_write()?;
    let fields = schema().validate(input.clone(), false)?;
    let actor = principal.cursor_scope();
    let scope = tenant.key();
    let key = headers
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(bracel_data::conflict)?;
    let result = match Mutation::begin(
        &db,
        &format!("{scope}:{actor}"),
        "tenant.projects.create",
        key,
        &input,
    )
    .await?
    {
        Attempt::Replay(result) => result,
        Attempt::New(mutation) => {
            bracel_data::TenantScope::resolve(&mutation.transaction, id, &principal)
                .await?
                .require_write()?;
            let document = insert_project(&mutation.transaction, &scope, &actor, fields).await?;
            mutation.finish(201, json!({"data":document})).await?
        }
    };
    Ok((StatusCode::CREATED, Json(result.body)))
}
