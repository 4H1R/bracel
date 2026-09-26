use super::{ISSUER, SCOPE, SESSION_SECONDS, dto::*};
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier, password_hash::SaltString};
use axum::http::StatusCode;
use bracel::{http::error::AppError, identity::Principal};
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DbBackend, QueryResult, Statement, TransactionTrait,
};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tokio::sync::Semaphore;
use uuid::Uuid;

pub(super) fn sql(query: &str, values: Vec<sea_orm::Value>) -> Statement {
    Statement::from_sql_and_values(DbBackend::Postgres, query, values)
}
pub(super) fn hash(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}
#[cfg(feature = "mail")]
pub(super) fn secret() -> Result<String, AppError> {
    let mut bytes = [0; 32];
    getrandom::fill(&mut bytes).map_err(|_| unavailable())?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
pub(super) fn unavailable() -> AppError {
    AppError::new(
        StatusCode::SERVICE_UNAVAILABLE,
        "Account service unavailable",
    )
}
pub(super) fn denied() -> AppError {
    AppError::new(StatusCode::UNAUTHORIZED, "Invalid credentials")
}
fn user(row: &QueryResult) -> Result<User, AppError> {
    Ok(User {
        id: row.try_get("", "id")?,
        email: row.try_get("", "email")?,
        display_name: row.try_get("", "display_name")?,
    })
}
async fn quota(
    db: &DatabaseConnection,
    action: &str,
    email: &str,
    max: i32,
) -> Result<(), AppError> {
    let row=db.query_one_raw(sql("INSERT INTO account_quotas(key,attempts,expires_at) VALUES($1,1,clock_timestamp()+interval '15 minutes')
        ON CONFLICT(key) DO UPDATE SET attempts=CASE WHEN account_quotas.expires_at<=clock_timestamp() THEN 1 ELSE account_quotas.attempts+1 END,
        expires_at=CASE WHEN account_quotas.expires_at<=clock_timestamp() THEN clock_timestamp()+interval '15 minutes' ELSE account_quotas.expires_at END RETURNING attempts",
        vec![hash(&format!("{action}:{email}")).into()])).await?.ok_or_else(unavailable)?;
    if row.try_get::<i32>("", "attempts")? > max {
        return Err(AppError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "Too many account requests; try again later",
        ));
    }
    Ok(())
}
async fn password_work(
    gate: Arc<Semaphore>,
    password: String,
    existing: Option<String>,
) -> Result<(String, bool), AppError> {
    let permit = gate.try_acquire_owned().map_err(|_| unavailable())?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let algorithm = Argon2::default();
        if let Some(existing) = existing {
            let parsed = PasswordHash::new(&existing).map_err(|_| unavailable())?;
            let valid = algorithm
                .verify_password(password.as_bytes(), &parsed)
                .is_ok();
            return Ok((existing, valid));
        }
        let mut salt = [0; 16];
        getrandom::fill(&mut salt).map_err(|_| unavailable())?;
        let salt = SaltString::encode_b64(&salt).map_err(|_| unavailable())?;
        let hash = algorithm
            .hash_password(password.as_bytes(), &salt)
            .map_err(|_| unavailable())?
            .to_string();
        Ok((hash, false))
    })
    .await
    .map_err(|_| unavailable())?
}
async fn session(db: &impl ConnectionTrait, user: User) -> Result<Session, AppError> {
    let principal = Principal::new(ISSUER.into(), user.id.to_string(), SCOPE.into())
        .map_err(|_| unavailable())?;
    let token = bracel::tokens::issue(db, &principal, SESSION_SECONDS).await?;
    db.execute_raw(sql(
        "INSERT INTO account_sessions(token_id,user_id) VALUES($1,$2)",
        vec![token.id.into(), user.id.into()],
    ))
    .await?;
    Ok(Session {
        user,
        access_token: token.secret,
        token_type: "Bearer",
        expires_in: SESSION_SECONDS,
    })
}
pub async fn register(
    db: &DatabaseConnection,
    gate: Arc<Semaphore>,
    input: Registration,
) -> Result<Session, AppError> {
    let email = email(&input.email)?;
    let display_name = name(&input.display_name)?;
    password(&input.password)?;
    quota(db, "register", &email, 5).await?;
    let (password_hash, _) = password_work(gate, input.password, None).await?;
    let tx = db.begin().await?;
    let id = Uuid::now_v7();
    let row=tx.query_one_raw(sql("INSERT INTO users(id,email,display_name,password_hash) VALUES($1,$2,$3,$4) ON CONFLICT(email) DO NOTHING RETURNING id,email,display_name",
        vec![id.into(),email.into(),display_name.into(),password_hash.into()])).await?
        .ok_or_else(|| AppError::new(StatusCode::CONFLICT,"An account with that email already exists"))?;
    let result = session(&tx, user(&row)?).await?;
    tx.commit().await?;
    Ok(result)
}
pub async fn login(
    db: &DatabaseConnection,
    gate: Arc<Semaphore>,
    input: Login,
) -> Result<Session, AppError> {
    let email = email(&input.email)?;
    if input.password.len() > 512 {
        return Err(denied());
    }
    quota(db, "login", &email, 10).await?;
    let row = db
        .query_one_raw(sql(
            "SELECT id,password_hash FROM users WHERE email=$1",
            vec![email.into()],
        ))
        .await?;
    let existing = row
        .as_ref()
        .map(|r| r.try_get::<String>("", "password_hash"))
        .transpose()?;
    let (verified_hash, valid) = password_work(gate, input.password, existing).await?;
    if !valid {
        return Err(denied());
    }
    let id: Uuid = row.ok_or_else(denied)?.try_get("", "id")?;
    let tx = db.begin().await?;
    let row = tx
        .query_one_raw(sql(
            "SELECT id,email,display_name,password_hash FROM users WHERE id=$1 FOR UPDATE",
            vec![id.into()],
        ))
        .await?
        .ok_or_else(denied)?;
    if row.try_get::<String>("", "password_hash")? != verified_hash {
        return Err(denied());
    }
    let result = session(&tx, user(&row)?).await?;
    tx.commit().await?;
    Ok(result)
}
fn subject(principal: &Principal) -> Result<Uuid, AppError> {
    if principal.iss != ISSUER {
        return Err(denied());
    }
    principal.sub.parse().map_err(|_| denied())
}
pub async fn me(db: &DatabaseConnection, principal: &Principal) -> Result<User, AppError> {
    let row = db
        .query_one_raw(sql(
            "SELECT id,email,display_name FROM users WHERE id=$1",
            vec![subject(principal)?.into()],
        ))
        .await?
        .ok_or_else(denied)?;
    user(&row)
}
pub async fn profile(
    db: &DatabaseConnection,
    principal: &Principal,
    input: Profile,
) -> Result<User, AppError> {
    let name = name(&input.display_name)?;
    let row = db
        .query_one_raw(sql(
            "UPDATE users SET display_name=$2 WHERE id=$1 RETURNING id,email,display_name",
            vec![subject(principal)?.into(), name.into()],
        ))
        .await?
        .ok_or_else(denied)?;
    user(&row)
}
pub async fn logout(
    db: &DatabaseConnection,
    principal: &Principal,
    token: &str,
    all: bool,
) -> Result<(), AppError> {
    let id = subject(principal)?;
    let tx = db.begin().await?;
    tx.query_one_raw(sql(
        "SELECT id FROM users WHERE id=$1 FOR UPDATE",
        vec![id.into()],
    ))
    .await?
    .ok_or_else(denied)?;
    tx.execute_raw(sql("UPDATE bracel_tokens t SET revoked_at=clock_timestamp() FROM account_sessions s WHERE s.token_id=t.id AND s.user_id=$1 AND ($2 OR t.token_hash=$3) AND t.revoked_at IS NULL",vec![id.into(),all.into(),hash(token).into()])).await?;
    tx.commit().await?;
    Ok(())
}
pub async fn forgot(db: &DatabaseConnection, input: Forgot) -> Result<(), AppError> {
    let email = email(&input.email)?;
    quota(db, "forgot", &email, 3).await?;
    let tx = db.begin().await?;
    if let Some(row) = tx
        .query_one_raw(sql(
            "SELECT id FROM users WHERE email=$1 FOR UPDATE",
            vec![email.into()],
        ))
        .await?
    {
        let id: Uuid = row.try_get("", "id")?;
        tx.execute_raw(sql("INSERT INTO account_resets(id,user_id,expires_at) VALUES($1,$2,clock_timestamp()+interval '15 minutes') ON CONFLICT(user_id) DO UPDATE SET id=excluded.id,token_hash=NULL,expires_at=excluded.expires_at,attempts=0,delivered=false,lease_id=NULL,lease_until=NULL,available_at=clock_timestamp()",vec![Uuid::now_v7().into(),id.into()])).await?;
    }
    tx.commit().await?;
    Ok(())
}
pub async fn reset(
    db: &DatabaseConnection,
    gate: Arc<Semaphore>,
    input: Reset,
) -> Result<(), AppError> {
    password(&input.password)?;
    let invalid = || AppError::new(StatusCode::BAD_REQUEST, "Invalid or expired reset token");
    if input.token.len() != 64 || !input.token.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(invalid());
    }
    let digest = hash(&input.token);
    let row=db.query_one_raw(sql("SELECT user_id FROM account_resets WHERE token_hash=$1 AND expires_at>clock_timestamp()",vec![digest.clone().into()])).await?.ok_or_else(invalid)?;
    let id: Uuid = row.try_get("", "user_id")?;
    let (password_hash, _) = password_work(gate, input.password, None).await?;
    let tx = db.begin().await?;
    tx.query_one_raw(sql(
        "SELECT id FROM users WHERE id=$1 FOR UPDATE",
        vec![id.into()],
    ))
    .await?
    .ok_or_else(invalid)?;
    let consumed=tx.execute_raw(sql("DELETE FROM account_resets WHERE user_id=$1 AND token_hash=$2 AND expires_at>clock_timestamp()",vec![id.into(),digest.into()])).await?.rows_affected();
    if consumed != 1 {
        return Err(invalid());
    }
    tx.execute_raw(sql(
        "UPDATE users SET password_hash=$2 WHERE id=$1",
        vec![id.into(), password_hash.into()],
    ))
    .await?;
    tx.execute_raw(sql("UPDATE bracel_tokens t SET revoked_at=clock_timestamp() FROM account_sessions s WHERE s.token_id=t.id AND s.user_id=$1 AND t.revoked_at IS NULL",vec![id.into()])).await?;
    tx.commit().await?;
    Ok(())
}
