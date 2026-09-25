use serde::Serialize;
use utoipa::ToSchema;

/// Success envelope for a single JSON resource. Errors keep the Problem contract.
#[derive(Serialize, ToSchema)]
pub struct Data<T> {
    pub data: T,
}

impl<T> Data<T> {
    pub fn new(data: T) -> Self {
        Self { data }
    }
}

#[derive(Serialize, ToSchema)]
pub struct Page<T> {
    pub data: Vec<T>,
    pub page: PageInfo,
}

#[derive(Serialize, ToSchema)]
pub struct PageInfo {
    pub limit: u64,
    pub has_more: bool,
    /// Null when there is no next page. Pass unchanged as the `after` query parameter.
    #[schema(required = true)]
    pub next_cursor: Option<String>,
}

impl<T> Page<T> {
    pub fn new(data: Vec<T>, limit: u64, next_cursor: Option<String>) -> Self {
        Self {
            data,
            page: PageInfo {
                limit,
                has_more: next_cursor.is_some(),
                next_cursor,
            },
        }
    }
}
