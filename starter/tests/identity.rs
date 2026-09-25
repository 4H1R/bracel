use bracel::{
    identity::{BearerAuth, Principal},
    testing::{TestClient, TestDatabase},
    tokens,
};
use bracel_starter::{AppState, app, config::Config};
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use sea_orm::{ConnectionTrait, DbBackend, Statement};
use serde_json::json;
use std::collections::BTreeMap;
const PUBLIC: &str = include_str!("fixtures/test-only-public.pem");
fn token(kid: Option<&str>) -> String {
    let mut header = Header::new(Algorithm::RS256);
    header.typ = Some("at+jwt".into());
    header.kid = kid.map(Into::into);
    jsonwebtoken::encode(&header,&json!({"iss":"test","sub":"one","aud":"api","scope":"notes:read","exp":jsonwebtoken::get_current_timestamp()+300}),
        &EncodingKey::from_rsa_pem(include_bytes!("fixtures/test-only-private.pem")).unwrap()).unwrap()
}
#[test]
fn rotation_is_atomic_requires_kid_and_updates_clones() {
    let keys = |name: &str| BTreeMap::from([(name.to_string(), PUBLIC.to_owned())]);
    let verifier = BearerAuth::from_keys(keys("old"), "test", "api").unwrap();
    let clone = verifier.clone();
    assert!(verifier.verify(&token(Some("old"))).is_some());
    assert!(verifier.verify(&token(None)).is_none());
    assert!(verifier.verify(&token(Some("unknown"))).is_none());
    assert!(
        verifier
            .replace_keys(BTreeMap::from([("bad".into(), "invalid".into())]))
            .is_err()
    );
    assert!(clone.verify(&token(Some("old"))).is_some());
    verifier
        .replace_keys(BTreeMap::from([
            ("old".into(), PUBLIC.into()),
            ("new".into(), PUBLIC.into()),
        ]))
        .unwrap();
    assert!(clone.verify(&token(Some("new"))).is_some());
    verifier.replace_keys(keys("new")).unwrap();
    assert!(clone.verify(&token(Some("old"))).is_none());
    assert!(clone.verify(&token(Some("new"))).is_some());
}
#[tokio::test]
async fn machine_tokens_store_only_hashes_and_enforce_scope_expiry_and_revocation() {
    let url = std::env::var("TEST_DATABASE_URL").unwrap();
    let db = TestDatabase::connect(&url).await.unwrap();
    db.db.execute_unprepared(tokens::SCHEMA).await.unwrap();
    let principal = Principal::new("test".into(), "machine".into(), "notes:read".into()).unwrap();
    let issued = tokens::issue(&db.db, &principal, 60).await.unwrap();
    let stored = db
        .db
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT token_hash FROM bracel_tokens",
        ))
        .await
        .unwrap()
        .unwrap()
        .try_get::<String>("", "token_hash")
        .unwrap();
    assert_eq!(stored.len(), 64);
    assert!(!issued.secret.contains(&stored));
    assert_eq!(
        tokens::verify(&db.db, &issued.secret)
            .await
            .unwrap()
            .unwrap()
            .sub,
        "machine"
    );
    let config = Config::from_lookup(|key| match key {
        "DATABASE_URL" => Some(url.clone()),
        "AUTH_MODE" => Some("bearer".into()),
        "AUTH_PUBLIC_KEY_PEM" => Some(PUBLIC.into()),
        "AUTH_ISSUER" => Some("test".into()),
        "AUTH_AUDIENCE" => Some("api".into()),
        "AUTH_MACHINE_TOKENS" => Some("true".into()),
        "ENABLE_EXAMPLE" => Some("true".into()),
        _ => None,
    })
    .unwrap();
    let client =
        TestClient::new(app(AppState { db: db.db.clone() }, &config)).bearer(&issued.secret);
    client
        .request("POST", "/example/notes", Some(json!({"title":"denied"})))
        .await
        .assert_status(403);
    assert!(tokens::revoke(&db.db, issued.id).await.unwrap());
    assert!(
        tokens::verify(&db.db, &issued.secret)
            .await
            .unwrap()
            .is_none()
    );
    client
        .request("GET", "/example/notes", None)
        .await
        .assert_status(401);
    let expired = tokens::issue(&db.db, &principal, 60).await.unwrap();
    db.db
        .execute_unprepared(
            "UPDATE bracel_tokens SET expires_at=clock_timestamp()-interval '1 second'",
        )
        .await
        .unwrap();
    assert!(
        tokens::verify(&db.db, &expired.secret)
            .await
            .unwrap()
            .is_none()
    );
    assert!(tokens::issue(&db.db, &principal, 0).await.is_err());
    db.cleanup().await.unwrap();
}
