use super::{
    application,
    dto::{CreateNote, Note, NoteInput},
};
use crate::http::{
    pagination::{PageQuery, encode_cursor},
    response::Page,
};
use crate::{
    AppState,
    http::{error::AppError, response::Data},
};
use axum::{Extension, extract::RawQuery};
use axum::{Json, extract::State, http::StatusCode};
use bracel::http::extract::{TypedPath, ValidatedJson};
use uuid::Uuid;
pub(crate) const READ_SCOPE: &str = "notes:read";
pub(crate) const WRITE_SCOPE: &str = "notes:write";

pub(crate) fn register(registry: &mut bracel::http::registry::Registry<AppState>, enabled: bool) {
    use bracel::{http::registry::RoutePolicy, utoipa_axum::routes};
    registry.register(
        routes!(list),
        RoutePolicy::Example(READ_SCOPE),
        enabled,
        super::query::parameters(),
    );
    registry.register(
        routes!(get_note),
        RoutePolicy::Example(READ_SCOPE),
        enabled,
        vec![],
    );
    registry.register(
        routes!(create),
        RoutePolicy::Example(WRITE_SCOPE),
        enabled,
        vec![],
    );
}
#[utoipa::path(post, path = "/example/notes", request_body = CreateNote, responses(
    (status = 201, description = "Created note", body = Data<Note>),
    (status = 400, description = "Malformed JSON", body = crate::http::error::Problem, content_type = "application/problem+json"),
    (status = 413, description = "Body too large", body = crate::http::error::Problem, content_type = "application/problem+json"),
    (status = 415, description = "Expected JSON", body = crate::http::error::Problem, content_type = "application/problem+json"),
    (status = 422, description = "Invalid note", body = crate::http::error::Problem, content_type = "application/problem+json"),
    (status = 408, description = "Deadline exceeded", body = crate::http::error::Problem, content_type = "application/problem+json"),
    (status = 503, description = "Database unavailable", body = crate::http::error::Problem, content_type = "application/problem+json")
))]
pub async fn create(
    State(state): State<AppState>,
    ValidatedJson(input): ValidatedJson<NoteInput>,
) -> Result<(StatusCode, Json<Data<Note>>), AppError> {
    Ok((
        StatusCode::CREATED,
        Json(Data::new(application::create_note(&state.db, input).await?)),
    ))
}

#[utoipa::path(get, path = "/example/notes/{id}", params(("id" = Uuid, Path, description = "Note ID")), responses(
    (status = 200, description = "Note", body = Data<Note>),
    (status = 400, description = "Invalid ID", body = crate::http::error::Problem, content_type = "application/problem+json"),
    (status = 404, description = "Missing note", body = crate::http::error::Problem, content_type = "application/problem+json"),
    (status = 408, description = "Deadline exceeded", body = crate::http::error::Problem, content_type = "application/problem+json"),
    (status = 503, description = "Database unavailable", body = crate::http::error::Problem, content_type = "application/problem+json")
))]
pub async fn get_note(
    State(state): State<AppState>,
    TypedPath(id): TypedPath<Uuid>,
) -> Result<Json<Data<Note>>, AppError> {
    Ok(Json(Data::new(application::get_note(&state.db, id).await?)))
}

#[utoipa::path(get, path = "/example/notes", params(PageQuery), responses(
    (status = 200, description = "Notes ordered by creation time and ID descending", body = Page<Note>),
    (status = 400, description = "Malformed or unsupported query parameters", body = crate::http::error::Problem, content_type = "application/problem+json"),
    (status = 422, description = "Invalid limit or cursor", body = crate::http::error::Problem, content_type = "application/problem+json"),
    (status = 408, description = "Deadline exceeded", body = crate::http::error::Problem, content_type = "application/problem+json"),
    (status = 503, description = "Database unavailable", body = crate::http::error::Problem, content_type = "application/problem+json")
))]
pub async fn list(
    State(state): State<AppState>,
    RawQuery(query): RawQuery,
    principal: Option<Extension<crate::features::identity::Principal>>,
) -> Result<Json<Page<Note>>, AppError> {
    let access_scope = principal
        .as_ref()
        .map(|p| p.0.cursor_scope())
        .unwrap_or_else(|| "public".into());
    let input = application::ListNotes::parse(query.as_deref().unwrap_or(""), &access_scope)?;
    let page = application::list_notes(&state.db, &input).await?;
    let next = page
        .next
        .map(|position| encode_cursor(position, input.cursor_scope()))
        .transpose()?;
    Ok(Json(Page::new(page.items, input.limit(), next)))
}
