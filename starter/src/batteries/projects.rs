use super::*;

pub(super) async fn create(
    State(db): State<DatabaseConnection>,
    Extension(principal): Extension<Principal>,
    headers: HeaderMap,
    Extension(providers): Extension<crate::provider_settings::Providers>,
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
            let document =
                insert_project(&mutation.transaction, &scope, &scope, fields, &providers).await?;
            mutation.finish(201, json!({"data":document})).await?
        }
    };
    Ok((
        StatusCode::from_u16(result.status).map_err(|_| bracel_data::conflict())?,
        Json(result.body),
    ))
}

pub(super) async fn incoming_webhook(
    State(db): State<DatabaseConnection>,
    headers: HeaderMap,
    Extension(providers): Extension<crate::provider_settings::Providers>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, AppError> {
    let secret = providers.incoming_secret.as_deref().ok_or_else(|| {
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

pub(super) async fn notifications(
    State(db): State<DatabaseConnection>,
    Extension(principal): Extension<Principal>,
) -> Result<Json<Value>, AppError> {
    Ok(Json(
        json!({"data":bracel_delivery::inbox(&db,&principal.cursor_scope(),None,50).await?}),
    ))
}
pub(super) async fn notification_read(
    State(db): State<DatabaseConnection>,
    Extension(principal): Extension<Principal>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    bracel_delivery::mark_read(&db, &principal.cursor_scope(), id).await?;
    Ok(Json(json!({"data":{"read":true}})))
}
pub(super) async fn update(
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
    providers: &crate::provider_settings::Providers,
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
    if providers.webhook_url.is_some() {
        bracel_delivery::webhook::enqueue(
            tx,
            "primary",
            id,
            json!({"type":"project.created","id":id}),
        )
        .await?;
    }
    if let Some(to) = &providers.notify_email {
        bracel_delivery::enqueue_mail(
            tx,
            bracel_delivery::MailIntent {
                scope,
                dedupe: &format!("project-mail:{id}"),
                from: "notifications@example.test",
                to,
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

pub(super) async fn import_projects(
    State(db): State<DatabaseConnection>,
    Extension(principal): Extension<Principal>,
    headers: HeaderMap,
    Extension(providers): Extension<crate::provider_settings::Providers>,
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
                documents.push(
                    insert_project(&mutation.transaction, &scope, &scope, fields, &providers)
                        .await?,
                );
            }
            mutation.finish(201, json!({"data":documents})).await?
        }
    };
    Ok((StatusCode::CREATED, Json(result.body)))
}

pub(super) async fn tenant_projects(
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
pub(super) async fn tenant_create(
    State(db): State<DatabaseConnection>,
    Extension(principal): Extension<Principal>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Extension(providers): Extension<crate::provider_settings::Providers>,
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
            let document =
                insert_project(&mutation.transaction, &scope, &actor, fields, &providers).await?;
            mutation.finish(201, json!({"data":document})).await?
        }
    };
    Ok((StatusCode::CREATED, Json(result.body)))
}

pub(super) fn missing() -> AppError {
    AppError::new(StatusCode::NOT_FOUND, "Project not found")
}
pub(super) fn representation(row: QueryResult) -> Result<Value, AppError> {
    let mut value: Value = serde_json::from_str(&row.try_get::<String>("", "document")?)
        .map_err(|_| bracel_data::conflict())?;
    value["id"] = json!(row.try_get::<Uuid>("", "id")?);
    value["version"] = json!(row.try_get::<i64>("", "version")?);
    Ok(value)
}
pub(super) async fn read(
    State(db): State<DatabaseConnection>,
    Extension(principal): Extension<Principal>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let row=db.query_one_raw(sql("SELECT id,version,document::text AS document FROM projects WHERE id=$1 AND owner=$2 AND deleted_at IS NULL",vec![id.into(),principal.cursor_scope().into()])).await?.ok_or_else(missing)?;
    Ok(Json(json!({"data":representation(row)?})))
}
