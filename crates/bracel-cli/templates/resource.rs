use axum::{Json, Extension, extract::{State, RawQuery}, http::StatusCode};
use bracel::{authorization::{owned, owner_key}, identity::Principal, http::{
    error::AppError, extract::{Validate, ValidatedJson, TypedPath, UniqueJson}, schema::{Schema,Field,Rule}, fields::Fields,
    pagination::{PageRequest, PageQuery, encode_cursor, invalid_cursor},
    registry::{Registry, RoutePolicy}, response::{Data, Page}},
    query::{QuerySpec, Filter, FilterKind, Sort}, utoipa_axum::routes};
use sea_orm::{prelude::*, Set, QuerySelect, QueryFilter, sea_query::{Expr, ExprTrait}};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;
use crate::AppState;

mod entity {
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
#[derive(Serialize, Deserialize, ToSchema)]
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


fn schema()->Schema {Schema(vec![__SCHEMA_FIELDS__])}

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
    registry.register(routes!(patch), RoutePolicy::Scope("__TABLE__:write"), true, vec![]);
    registry.register(routes!(delete), RoutePolicy::Scope("__TABLE__:write"), true, vec![]);
    for method in ["post","put","patch"] {
        let path=if method=="post" {"/__TABLE__"} else {"/__TABLE__/{id}"};
        registry.request_schema(path,method,schema().openapi(method=="patch")).expect("generated request schema");
    }
}
fn not_found() -> AppError { AppError::new(StatusCode::NOT_FOUND, "Resource not found") }

#[utoipa::path(post, path = "/__TABLE__", operation_id="__TABLE___create", request_body = Write__TYPE__, responses(
    (status=201, body=Data<__TYPE__>), (status=422, body=bracel::http::error::Problem, content_type="application/problem+json"),
    (status=400, body=bracel::http::error::Problem, content_type="application/problem+json"), (status=413, body=bracel::http::error::Problem, content_type="application/problem+json"),
    (status=415, body=bracel::http::error::Problem, content_type="application/problem+json")))]
pub async fn create(State(state): State<AppState>, Extension(principal): Extension<Principal>,
    ValidatedJson(input): ValidatedJson<Input>) -> Result<(StatusCode, Json<Data<__TYPE__>>), AppError> {
    Ok((StatusCode::CREATED, Json(Data::new(create_record(&state.db, &principal, input).await?))))
}
#[utoipa::path(get, path = "/__TABLE__/{id}", operation_id="__TABLE___show", params(("id"=Uuid, Path)), responses(
    (status=200, body=Data<__TYPE__>), (status=404, body=bracel::http::error::Problem, content_type="application/problem+json"),
    (status=400, body=bracel::http::error::Problem, content_type="application/problem+json")))]
pub async fn show(State(state): State<AppState>, Extension(principal): Extension<Principal>,
    TypedPath(id): TypedPath<Uuid>) -> Result<Json<Data<__TYPE__>>, AppError> {
    Ok(Json(Data::new(get_record(&state.db, &principal, id).await?)))
}
#[utoipa::path(put, path = "/__TABLE__/{id}", operation_id="__TABLE___update", params(("id"=Uuid, Path)), request_body = Write__TYPE__, responses(
    (status=200, body=Data<__TYPE__>), (status=404, body=bracel::http::error::Problem, content_type="application/problem+json"),
    (status=422, body=bracel::http::error::Problem, content_type="application/problem+json")))]
pub async fn update(State(state): State<AppState>, Extension(principal): Extension<Principal>,
    TypedPath(id): TypedPath<Uuid>, ValidatedJson(input): ValidatedJson<Input>) -> Result<Json<Data<__TYPE__>>, AppError> {
    Ok(Json(Data::new(update_record(&state.db, &principal, id, input).await?)))
}
#[utoipa::path(patch,path="/__TABLE__/{id}",operation_id="__TABLE___patch",params(("id"=Uuid,Path)),request_body=Write__TYPE__,responses((status=200,body=Data<__TYPE__>),(status=422,body=bracel::http::error::Problem,content_type="application/problem+json")))]
pub async fn patch(State(state):State<AppState>,Extension(principal):Extension<Principal>,TypedPath(id):TypedPath<Uuid>,UniqueJson(value):UniqueJson)->Result<Json<Data<__TYPE__>>,AppError>{
    use sea_orm::TransactionTrait;
    let tx = state.db.begin().await?;
    let record = patch_record(&tx, &principal, id, value).await?;
    tx.commit().await?;
    Ok(Json(Data::new(record)))
}
#[utoipa::path(delete, path = "/__TABLE__/{id}", operation_id="__TABLE___delete", params(("id"=Uuid, Path)), responses(
    (status=204, description="Deleted"), (status=404, body=bracel::http::error::Problem, content_type="application/problem+json")))]
pub async fn delete(State(state): State<AppState>, Extension(principal): Extension<Principal>,
    TypedPath(id): TypedPath<Uuid>) -> Result<StatusCode, AppError> {
    delete_record(&state.db, &principal, id).await?;
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
    Ok(Json(list_records(&state.db, &principal, raw.as_deref().unwrap_or("")).await?))
}

pub use application::{create_record, get_record, update_record, patch_record, delete_record, list_records};
mod application {
    use super::*;
    fn require(principal: &Principal, scope: &str) -> Result<(), AppError> {
        if principal.allows(scope) { Ok(()) } else { Err(AppError::new(StatusCode::FORBIDDEN, "Required permission is missing")) }
    }
    fn validate(input: &Write__TYPE__) -> Result<(), AppError> {
        let value = serde_json::to_value(input).map_err(|_| AppError::new(StatusCode::UNPROCESSABLE_ENTITY, "Invalid resource"))?;
        schema().validate(value, false)?;
        Ok(())
    }
pub async fn create_record(db: &impl ConnectionTrait, principal: &Principal, input: Write__TYPE__) -> Result<__TYPE__, AppError> {
    require(principal, "__TABLE__:write")?;
    validate(&input)?;

    let row = entity::ActiveModel { id: Set(Uuid::now_v7()), owner: Set(owner_key(principal)),
        __SET__, ..Default::default() }.insert(db).await?;
    Ok(row.into())
}
pub async fn get_record(db: &impl ConnectionTrait, principal: &Principal, id: Uuid) -> Result<__TYPE__, AppError> {
    require(principal, "__TABLE__:read")?;

    let row = owned(Entity::find_by_id(id), Column::Owner, principal).one(db).await?.ok_or_else(not_found)?;
    Ok(row.into())
}
pub async fn update_record(db: &impl ConnectionTrait, principal: &Principal, id: Uuid, input: Write__TYPE__) -> Result<__TYPE__, AppError> {
    require(principal, "__TABLE__:write")?;
    validate(&input)?;

    // The owner predicate is part of the write, so a concurrent ownership change cannot bypass it.
    let mut rows = Entity::update_many()
__UPDATE__
        .filter(Column::Id.eq(id)).filter(Column::Owner.eq(owner_key(principal)))
        .exec_with_returning(db).await?;
    Ok(rows.pop().ok_or_else(not_found)?.into())
}
pub async fn patch_record(tx: &sea_orm::DatabaseTransaction, principal: &Principal, id: Uuid, value: serde_json::Value) -> Result<__TYPE__, AppError> {
    require(principal, "__TABLE__:write")?;

    use sea_orm::sea_query::LockType;
    let patch=schema().validate(value,true)?;
    let row=owned(Entity::find_by_id(id),Column::Owner,principal).lock(LockType::Update).one(tx).await?.ok_or_else(not_found)?;
    let mut body=serde_json::to_value(__TYPE__::from(row)).map_err(|_|not_found())?;
    body.as_object_mut().expect("resource object").extend(patch);
    let input:Write__TYPE__=serde_json::from_value(body).map_err(|_|not_found())?;
    let mut rows=Entity::update_many()
__UPDATE__
        .filter(Column::Id.eq(id)).filter(Column::Owner.eq(owner_key(principal))).exec_with_returning(tx).await?;
    Ok(rows.pop().ok_or_else(not_found)?.into())
}
pub async fn delete_record(db: &impl ConnectionTrait, principal: &Principal, id: Uuid) -> Result<(), AppError> {
    require(principal, "__TABLE__:write")?;

    let result = Entity::delete_many().filter(Column::Id.eq(id)).filter(Column::Owner.eq(owner_key(principal))).exec(db).await?;
    if result.rows_affected == 0 { return Err(not_found()); }
    Ok(())
}
pub async fn list_records(db: &impl ConnectionTrait, principal: &Principal, raw: &str) -> Result<Page<__TYPE__>, AppError> {
    require(principal, "__TABLE__:read")?;

    let mut filter = spec().parse(bracel::http::query::decode(raw)?, &owner_key(principal))?;
    let page = PageRequest::<Cursor>::parse(std::mem::take(&mut filter.page), &filter.scope)?;
    let mut query = filter.apply(owned(Entity::find(), Column::Owner, principal));
    if let Some(after) = page.after() {
        if !bracel::query::is_supported_timestamp(&after.created_at) { return Err(invalid_cursor()); }
        let columns = Expr::tuple([Expr::col(Column::CreatedAt), Expr::col(Column::Id)]);
        let values = Expr::tuple([Expr::val(after.created_at), Expr::val(after.id)]);
        query = query.filter(if filter.descending { columns.lt(values) } else { columns.gt(values) });
    }
    let mut rows = query.limit(page.limit()+1).all(db).await?;
    let more = rows.len() > page.limit() as usize;
    rows.truncate(page.limit() as usize);
    let next = if more { rows.last().map(|row| encode_cursor(Cursor { created_at: row.created_at, id: row.id }, &filter.scope)).transpose()? } else { None };
    Ok(Page::new(rows.into_iter().map(Into::into).collect(), page.limit(), next))
}
}
