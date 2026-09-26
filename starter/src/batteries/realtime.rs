use super::*;

#[derive(Clone)]
pub(super) struct RealtimeState {
    pub(super) events: bracel_realtime::EventStore,
    pub(super) auth: bracel::identity::BearerAuth,
    pub(super) origins: Vec<axum::http::HeaderValue>,
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SubscriptionQuery {
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
pub(super) async fn events_sse(
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
pub(super) async fn events_socket(
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
