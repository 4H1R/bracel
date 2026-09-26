use super::*;

pub(super) async fn file_init(
    Extension(files): Extension<bracel_files::Files>,
    Extension(principal): Extension<Principal>,
    Json(input): Json<bracel_files::Upload>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    Ok((
        StatusCode::CREATED,
        Json(json!({"data":files.initialize(&principal.cursor_scope(),input).await?})),
    ))
}
pub(super) async fn file_metadata(
    Extension(files): Extension<bracel_files::Files>,
    Extension(principal): Extension<Principal>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    Ok(Json(
        json!({"data":files.metadata(&principal.cursor_scope(),id).await?}),
    ))
}
pub(super) async fn file_upload(
    Extension(files): Extension<bracel_files::Files>,
    Extension(principal): Extension<Principal>,
    Path(id): Path<Uuid>,
    body: axum::body::Bytes,
) -> Result<Json<Value>, AppError> {
    Ok(Json(
        json!({"data":files.upload(&principal.cursor_scope(),id,body.to_vec()).await?}),
    ))
}
pub(super) async fn file_complete(
    Extension(files): Extension<bracel_files::Files>,
    Extension(principal): Extension<Principal>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    Ok(Json(
        json!({"data":files.complete(&principal.cursor_scope(),id).await?}),
    ))
}
pub(super) async fn file_delete(
    Extension(files): Extension<bracel_files::Files>,
    Extension(principal): Extension<Principal>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    files.delete(&principal.cursor_scope(), id).await?;
    Ok(StatusCode::NO_CONTENT)
}
pub(super) async fn file_download(
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
