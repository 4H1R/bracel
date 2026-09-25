use super::entity::Column;
use crate::query::{Filter, FilterKind, QuerySpec, Sort};

pub(super) fn spec() -> QuerySpec<Column> {
    QuerySpec {
        resource: "notes:v2",
        filters: vec![
            Filter {
                name: "id",
                column: Column::Id,
                kind: FilterKind::Uuid,
            },
            Filter {
                name: "title",
                column: Column::Title,
                kind: FilterKind::TextContains,
            },
            Filter {
                name: "title_exact",
                column: Column::Title,
                kind: FilterKind::TextExact,
            },
            Filter {
                name: "created_after",
                column: Column::CreatedAt,
                kind: FilterKind::TimestampFrom,
            },
            Filter {
                name: "created_before",
                column: Column::CreatedAt,
                kind: FilterKind::TimestampTo,
            },
        ],
        sorts: vec![Sort {
            name: "created_at",
            columns: vec![Column::CreatedAt, Column::Id],
        }],
        default_sort: "-created_at",
    }
}

pub(crate) fn parameters() -> Vec<serde_json::Value> {
    spec().parameters()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sea_orm::{ColumnTrait, DbBackend, EntityTrait, QueryFilter, QueryTrait};
    #[test]
    fn user_filters_cannot_replace_the_mandatory_base_scope() {
        let id = uuid::Uuid::now_v7();
        let base = super::super::entity::Entity::find().filter(Column::Id.eq(id));
        let input = spec()
            .parse(
                crate::http::query::decode("filter[title]=hello")
                    .ok()
                    .unwrap(),
                "tenant:123",
            )
            .ok()
            .unwrap();
        let sql = input.apply(base).build(DbBackend::Postgres).to_string();
        assert!(sql.contains(&id.to_string()));
        assert!(sql.contains("AND"));
        assert!(sql.contains("LIKE"));
    }
}
