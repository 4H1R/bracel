//! Opaque machine tokens: the secret is returned once; only its digest is stored.
use crate::identity::Principal;
use sea_orm::{ConnectionTrait, DbBackend, DbErr, Statement};
use sha2::{Digest, Sha256};
use uuid::Uuid;
/// Initial migration SQL, frozen after release. Add separately named upgrade SQL for future schema changes.
pub const SCHEMA: &str = "CREATE TABLE bracel_tokens (
    id uuid PRIMARY KEY, token_hash text NOT NULL UNIQUE, issuer text NOT NULL, subject text NOT NULL, scope text NOT NULL,
    expires_at timestamptz NOT NULL, revoked_at timestamptz, created_at timestamptz NOT NULL DEFAULT clock_timestamp());";
pub struct IssuedToken {
    pub id: Uuid,
    pub secret: String,
}
fn hash(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}
pub async fn issue(
    db: &impl ConnectionTrait,
    principal: &Principal,
    lifetime_seconds: i32,
) -> Result<IssuedToken, DbErr> {
    if !(1..=31536000).contains(&lifetime_seconds) {
        return Err(DbErr::Custom(
            "Token lifetime must be 1..31536000 seconds".into(),
        ));
    }
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes)
        .map_err(|_| DbErr::Custom("Token generation unavailable".into()))?;
    let secret = format!(
        "brc_{}",
        bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
    );
    let id = Uuid::now_v7();
    db.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,
        "INSERT INTO bracel_tokens(id,token_hash,issuer,subject,scope,expires_at) VALUES($1,$2,$3,$4,$5,clock_timestamp()+$6*interval '1 second')",
        [id.into(),hash(&secret).into(),principal.iss.clone().into(),principal.sub.clone().into(),principal.scopes().into(),lifetime_seconds.into()])).await?;
    Ok(IssuedToken { id, secret })
}
pub async fn revoke(db: &impl ConnectionTrait, id: Uuid) -> Result<bool, DbErr> {
    Ok(db.execute_raw(Statement::from_sql_and_values(DbBackend::Postgres,"UPDATE bracel_tokens SET revoked_at=clock_timestamp() WHERE id=$1 AND revoked_at IS NULL",[id.into()])).await?.rows_affected()==1)
}
pub async fn verify(db: &impl ConnectionTrait, token: &str) -> Result<Option<Principal>, DbErr> {
    if token.len() != 68
        || !token.starts_with("brc_")
        || !token[4..].bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Ok(None);
    }
    let row=db.query_one_raw(Statement::from_sql_and_values(DbBackend::Postgres,
        "SELECT issuer,subject,scope,floor(extract(epoch FROM expires_at))::bigint AS expiry FROM bracel_tokens WHERE token_hash=$1 AND revoked_at IS NULL AND expires_at>clock_timestamp()",[hash(token).into()])).await?;
    row.map(|row| {
        let principal = Principal::new(
            row.try_get("", "issuer")?,
            row.try_get("", "subject")?,
            row.try_get("", "scope")?,
        )
        .map_err(|_| DbErr::Custom("Invalid token identity".into()))?;
        Ok(principal.with_expiration(row.try_get("", "expiry")?))
    })
    .transpose()
}
