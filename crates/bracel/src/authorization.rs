use crate::{http::error::AppError, identity::Principal};
use axum::http::StatusCode;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, Select};

/// Application policies can combine membership, state and action-specific rules.
pub trait Policy<R> {
    fn allows(&self, principal: &Principal, action: &str, resource: &R) -> bool;
}

pub fn authorize<R>(
    policy: &impl Policy<R>,
    principal: &Principal,
    action: &str,
    resource: &R,
) -> Result<(), AppError> {
    if policy.allows(principal, action, resource) {
        Ok(())
    } else {
        Err(AppError::new(
            StatusCode::FORBIDDEN,
            "Operation is not permitted",
        ))
    }
}

/// Persist this key in the owner column; issuer and subject are both significant.
pub fn owner_key(principal: &Principal) -> String {
    principal.cursor_scope()
}

/// Apply before filtering, pagination, lookup, update or delete to avoid disclosing other owners' records.
pub fn owned<E: EntityTrait>(
    query: Select<E>,
    owner_column: E::Column,
    principal: &Principal,
) -> Select<E> {
    query.filter(owner_column.eq(owner_key(principal)))
}
