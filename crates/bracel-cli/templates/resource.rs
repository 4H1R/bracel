use axum::{Json, Extension, extract::{State, RawQuery}, http::StatusCode};
use bracel::{authorization::{owned, owner_key}, identity::Principal, http::{
    error::AppError, extract::{Validate, ValidatedJson, TypedPath}, fields::Fields,
    pagination::{PageRequest, PageQuery, encode_cursor, invalid_cursor},
    registry::{Registry, RoutePolicy}, response::{Data, Page}},
    query::{QuerySpec, Filter, FilterKind, Sort}, utoipa_axum::routes};
use sea_orm::{prelude::*, Set, QuerySelect, QueryFilter, sea_query::{Expr, ExprTrait}};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;
use crate::AppState;

pub mod entity {
    use sea_orm::entity::prelude::*;
    #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
    #[sea_orm(table_name = "__TABLE__")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub id: Uuid,
        pub owner: String,
        pub created_at: DateTimeUtc,
__FIELDS__
    }
    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}
    impl ActiveModelBehavior for ActiveModel {}
}
use entity::{Entity, Column};

#[derive(Serialize, ToSchema)]
pub struct __TYPE__ {
    pub id: Uuid,
    #[schema(value_type = String, format = DateTime)]
    pub created_at: DateTimeUtc,
__FIELDS__
}
impl From<entity::Model> for __TYPE__ {
    fn from(row: entity::Model) -> Self {
        Self { id: row.id, created_at: row.created_at, __FROM__ }
    }
}
#[derive(Serialize, ToSchema)]
pub struct Write__TYPE__ {
__FIELDS__
}
#[derive(Deserialize)]
pub struct Input(Fields);
impl Validate for Input {
    type Output = Write__TYPE__;
    fn validate(self) -> Result<Self::Output, AppError> {
        let mut fields = self.0;
__VALIDATE__
        fields.finish()?;
        Ok(Write__TYPE__ { __NAMES__ })
    }
}
pub fn fixture() -> Write__TYPE__ {
    Write__TYPE__ { __FIXTURE__ }
}

fn spec() -> QuerySpec<Column> {
    QuerySpec {
        resource: "__TABLE__:v1",
        filters: vec![
            Filter { name: "id", column: Column::Id, kind: FilterKind::Uuid },
__FILTERS__
        ],
        sorts: vec![Sort { name: "created_at", columns: vec![Column::CreatedAt, Column::Id] }],
        default_sort: "-created_at",
    }
}
pub(crate) fn register(registry: &mut Registry<AppState>) {
    registry.register(routes!(list), RoutePolicy::Scope("__TABLE__:read"), true, spec().parameters());
    registry.register(routes!(show), RoutePolicy::Scope("__TABLE__:read"), true, vec![]);
    registry.register(routes!(create), RoutePolicy::Scope("__TABLE__:write"), true, vec![]);
    registry.register(routes!(update), RoutePolicy::Scope("__TABLE__:write"), true, vec![]);
    registry.register(routes!(delete), RoutePolicy::Scope("__TABLE__:write"), true, vec![]);
}
fn not_found() -> AppError { AppError::new(StatusCode::NOT_FOUND, "Resource not found") }

#[utoipa::path(post, path = "/__TABLE__", operation_id="__TABLE___create", request_body = Write__TYPE__, responses(
    (status=201, body=Data<__TYPE__>), (status=422, body=bracel::http::error::Problem, content_type="application/problem+json"),
    (status=400, body=bracel::http::error::Problem, content_type="application/problem+json"), (status=413, body=bracel::http::error::Problem, content_type="application/problem+json"),
    (status=415, body=bracel::http::error::Problem, content_type="application/problem+json")))]
pub async fn create(State(state): State<AppState>, Extension(principal): Extension<Principal>,
    ValidatedJson(input): ValidatedJson<Input>) -> Result<(StatusCode, Json<Data<__TYPE__>>), AppError> {
    let row = entity::ActiveModel { id: Set(Uuid::now_v7()), owner: Set(owner_key(&principal)),
        __SET__, ..Default::default() }.insert(&state.db).await?;
    Ok((StatusCode::CREATED, Json(Data::new(row.into()))))
}
#[utoipa::path(get, path = "/__TABLE__/{id}", operation_id="__TABLE___show", params(("id"=Uuid, Path)), responses(
    (status=200, body=Data<__TYPE__>), (status=404, body=bracel::http::error::Problem, content_type="application/problem+json"),
    (status=400, body=bracel::http::error::Problem, content_type="application/problem+json")))]
pub async fn show(State(state): State<AppState>, Extension(principal): Extension<Principal>,
    TypedPath(id): TypedPath<Uuid>) -> Result<Json<Data<__TYPE__>>, AppError> {
    let row = owned(Entity::find_by_id(id), Column::Owner, &principal).one(&state.db).await?.ok_or_else(not_found)?;
    Ok(Json(Data::new(row.into())))
}
#[utoipa::path(put, path = "/__TABLE__/{id}", operation_id="__TABLE___update", params(("id"=Uuid, Path)), request_body = Write__TYPE__, responses(
    (status=200, body=Data<__TYPE__>), (status=404, body=bracel::http::error::Problem, content_type="application/problem+json"),
    (status=422, body=bracel::http::error::Problem, content_type="application/problem+json")))]
pub async fn update(State(state): State<AppState>, Extension(principal): Extension<Principal>,
    TypedPath(id): TypedPath<Uuid>, ValidatedJson(input): ValidatedJson<Input>) -> Result<Json<Data<__TYPE__>>, AppError> {
    // The owner predicate is part of the write, so a concurrent ownership change cannot bypass it.
    let mut rows = Entity::update_many()
__UPDATE__
        .filter(Column::Id.eq(id)).filter(Column::Owner.eq(owner_key(&principal)))
        .exec_with_returning(&state.db).await?;
    Ok(Json(Data::new(rows.pop().ok_or_else(not_found)?.into())))
}
#[utoipa::path(delete, path = "/__TABLE__/{id}", operation_id="__TABLE___delete", params(("id"=Uuid, Path)), responses(
    (status=204, description="Deleted"), (status=404, body=bracel::http::error::Problem, content_type="application/problem+json")))]
pub async fn delete(State(state): State<AppState>, Extension(principal): Extension<Principal>,
    TypedPath(id): TypedPath<Uuid>) -> Result<StatusCode, AppError> {
    let result = Entity::delete_many().filter(Column::Id.eq(id)).filter(Column::Owner.eq(owner_key(&principal))).exec(&state.db).await?;
    if result.rows_affected == 0 { return Err(not_found()); }
    Ok(StatusCode::NO_CONTENT)
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor { created_at: DateTimeUtc, id: Uuid }
#[utoipa::path(get, path = "/__TABLE__", operation_id="__TABLE___list", params(PageQuery), responses(
    (status=200, body=Page<__TYPE__>), (status=400, body=bracel::http::error::Problem, content_type="application/problem+json"),
    (status=422, body=bracel::http::error::Problem, content_type="application/problem+json")))]
pub async fn list(State(state): State<AppState>, Extension(principal): Extension<Principal>,
    RawQuery(raw): RawQuery) -> Result<Json<Page<__TYPE__>>, AppError> {
    let mut filter = spec().parse(bracel::http::query::decode(raw.as_deref().unwrap_or(""))?, &owner_key(&principal))?;
    let page = PageRequest::<Cursor>::parse(std::mem::take(&mut filter.page), &filter.scope)?;
    let mut query = filter.apply(owned(Entity::find(), Column::Owner, &principal));
    if let Some(after) = page.after() {
        if !bracel::query::is_supported_timestamp(&after.created_at) { return Err(invalid_cursor()); }
        let columns = Expr::tuple([Expr::col(Column::CreatedAt), Expr::col(Column::Id)]);
        let values = Expr::tuple([Expr::val(after.created_at), Expr::val(after.id)]);
        query = query.filter(if filter.descending { columns.lt(values) } else { columns.gt(values) });
    }
    let mut rows = query.limit(page.limit()+1).all(&state.db).await?;
    let more = rows.len() > page.limit() as usize;
    rows.truncate(page.limit() as usize);
    let next = if more { rows.last().map(|row| encode_cursor(Cursor { created_at: row.created_at, id: row.id }, &filter.scope)).transpose()? } else { None };
    Ok(Json(Page::new(rows.into_iter().map(Into::into).collect(), page.limit(), next)))
}
