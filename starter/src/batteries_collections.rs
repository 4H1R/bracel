use super::*;
use axum::extract::RawQuery;
use bracel::http::collection::{Navigation, Selection, cursor};

pub async fn list(
    State(db): State<DatabaseConnection>,
    Extension(principal): Extension<Principal>,
    RawQuery(raw): RawQuery,
) -> Result<Json<Value>, AppError> {
    let mut query = bracel::http::query::decode(raw.as_deref().unwrap_or(""))?;
    let selection = Selection::parse(
        &mut query,
        &[
            "id",
            "version",
            "name",
            "active",
            "budget",
            "description",
            "labels",
            "due_at",
            "status",
        ],
        &["audit"],
    )?;
    let bad = || AppError::new(StatusCode::BAD_REQUEST, "Unsupported collection query");
    let active = query
        .remove("filter[active]")
        .map(|s| s.parse::<bool>().map_err(|_| bad()))
        .transpose()?;
    let status = query.remove("filter[status]");
    if status
        .as_deref()
        .is_some_and(|s| !matches!(s, "planned" | "active" | "done"))
    {
        return Err(bad());
    }
    let search = query.remove("q");
    if search.as_ref().is_some_and(|s| s.len() > 200) {
        return Err(bad());
    }
    let scope = principal.cursor_scope();
    let binding =
        bracel::http::collection::binding("projects", &(&scope, active, &status, &search))?;
    let navigation = Navigation::<Uuid>::parse(&mut query, &binding)?;
    if !query.is_empty() {
        return Err(bad());
    }
    let comparison = if navigation.backward { ">" } else { "<" };
    let order = if navigation.backward { "ASC" } else { "DESC" };
    let predicate = "owner=$1 AND deleted_at IS NULL AND ($2::boolean IS NULL OR (document->>'active')::boolean=$2) AND ($3::text IS NULL OR document->>'status'=$3) AND ($4::text IS NULL OR to_tsvector('simple',document->>'name') @@ websearch_to_tsquery('simple',$4))";
    let mut values = vec![
        scope.clone().into(),
        active.into(),
        status.into(),
        search.into(),
    ];
    let total = if navigation.offset.is_some() {
        Some(
            db.query_one_raw(sql(
                &format!("SELECT count(*) AS count FROM projects WHERE {predicate}"),
                values.clone(),
            ))
            .await?
            .ok_or_else(bad)?
            .try_get::<i64>("", "count")?,
        )
    } else {
        None
    };
    values.extend([
        navigation.position.into(),
        ((navigation.limit + 1) as i64).into(),
        (navigation.offset.unwrap_or(0) as i64).into(),
    ]);
    let rows=db.query_all_raw(sql(&format!("SELECT id,version,document::text AS document FROM projects WHERE {predicate} AND ($5::uuid IS NULL OR id {comparison} $5) ORDER BY id {order} LIMIT $6 OFFSET $7"),values)).await?;
    let mut documents = rows
        .into_iter()
        .map(representation)
        .collect::<Result<Vec<_>, _>>()?;
    let more = documents.len() > navigation.limit as usize;
    documents.truncate(navigation.limit as usize);
    if navigation.backward {
        documents.reverse();
    }
    let position = |value: &Value| {
        value["id"]
            .as_str()
            .and_then(|s| s.parse::<Uuid>().ok())
            .ok_or_else(bad)
    };
    let previous = if if navigation.backward {
        more
    } else {
        navigation.position.is_some() || navigation.offset.is_some_and(|n| n > 0)
    } {
        documents
            .first()
            .map(|v| cursor(position(v)?, &binding))
            .transpose()?
    } else {
        None
    };
    let next = if more || navigation.backward {
        documents
            .last()
            .map(|v| cursor(position(v)?, &binding))
            .transpose()?
    } else {
        None
    };
    if selection.includes.contains("audit") {
        let ids = documents
            .iter()
            .map(|d| d["id"].clone())
            .collect::<Vec<_>>();
        let rows=db.query_all_raw(sql("SELECT resource,action,metadata::text AS metadata FROM bracel_audit WHERE scope=$1 AND resource IN (SELECT value::uuid FROM jsonb_array_elements_text($2::jsonb)) ORDER BY created_at,id LIMIT 1000",vec![scope.into(),json!(ids).to_string().into()])).await?;
        let mut audits = std::collections::BTreeMap::<String, Vec<Value>>::new();
        for row in rows {
            audits
                .entry(row.try_get::<Uuid>("", "resource")?.to_string())
                .or_default()
                .push(json!({"action":row.try_get::<String>("","action")?}));
        }
        for document in &mut documents {
            document["audit"] = json!(
                audits
                    .remove(document["id"].as_str().unwrap_or(""))
                    .unwrap_or_default()
            );
        }
    }
    Ok(Json(
        json!({"data":documents.into_iter().map(|d|selection.project(d)).collect::<Vec<_>>(),"page":{"limit":navigation.limit,"has_more":next.is_some(),"next_cursor":next,"previous_cursor":previous,"total":total}}),
    ))
}
pub async fn snapshot(
    State(db): State<DatabaseConnection>,
    Extension(principal): Extension<Principal>,
) -> Result<Json<Value>, AppError> {
    let tx = db.begin().await?;
    tx.execute_unprepared("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .await?;
    let scope = principal.cursor_scope();
    let cursor = bracel_realtime::snapshot_cursor(&tx, &scope, "projects").await?;
    let rows=tx.query_all_raw(sql("SELECT id,version,document::text AS document FROM projects WHERE owner=$1 AND deleted_at IS NULL ORDER BY id LIMIT 1001",vec![scope.into()])).await?;
    if rows.len() > 1000 {
        return Err(AppError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "Snapshot exceeds 1000 resources; use a bounded export workflow",
        ));
    }
    let documents = rows
        .into_iter()
        .map(representation)
        .collect::<Result<Vec<_>, _>>()?;
    tx.commit().await?;
    Ok(Json(json!({"data":documents,"cursor":cursor})))
}
pub async fn delete(
    State(db): State<DatabaseConnection>,
    Extension(principal): Extension<Principal>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<StatusCode, AppError> {
    let expected =
        bracel_data::expected_version(headers.get("if-match").and_then(|h| h.to_str().ok()))?;
    let scope = principal.cursor_scope();
    let tx = db.begin().await?;
    let row=tx.query_one_raw(sql("SELECT version FROM projects WHERE id=$1 AND owner=$2 AND deleted_at IS NULL FOR UPDATE",vec![id.into(),scope.clone().into()])).await?.ok_or_else(missing)?;
    if row.try_get::<i64>("", "version")? != expected {
        return Err(bracel_data::stale());
    }
    tx.execute_raw(sql(
        "UPDATE projects SET deleted_at=clock_timestamp(),version=version+1 WHERE id=$1",
        vec![id.into()],
    ))
    .await?;
    bracel_data::audit(&tx, &scope, &scope, "project.deleted", id, json!({})).await?;
    bracel_realtime::publish_value(
        &tx,
        &scope,
        "projects",
        "project.deleted",
        1,
        json!({"id":id}),
    )
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
pub async fn restore(
    State(db): State<DatabaseConnection>,
    Extension(principal): Extension<Principal>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let scope = principal.cursor_scope();
    let tx = db.begin().await?;
    let row=tx.query_one_raw(sql("UPDATE projects SET deleted_at=NULL,version=version+1 WHERE id=$1 AND owner=$2 AND deleted_at IS NOT NULL RETURNING id,version,document::text AS document",vec![id.into(),scope.clone().into()])).await?.ok_or_else(missing)?;
    bracel_data::audit(&tx, &scope, &scope, "project.restored", id, json!({})).await?;
    bracel_realtime::publish_value(
        &tx,
        &scope,
        "projects",
        "project.restored",
        1,
        json!({"id":id}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({"data":representation(row)?})))
}
