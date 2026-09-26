use __CRATE__::{AppState, app, config::Config, migrations::Migrator};
use bracel::testing::{TestClient, TestDatabase};
use jsonwebtoken::{Header, Algorithm, EncodingKey};
use sea_orm_migration::MigratorTrait;
use serde_json::{Value, json};
fn token(subject: &str, scope: &str) -> String {
    let mut header = Header::new(Algorithm::RS256);
    header.typ = Some("at+jwt".into());
    jsonwebtoken::encode(&header, &json!({"sub":subject,"iss":"test","aud":"api","scope":scope,
        "exp":std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()+300}),
        &EncodingKey::from_rsa_pem(include_bytes!("fixtures/test-only-private.pem")).unwrap()).unwrap()
}
#[tokio::test]
async fn __TABLE___crud_is_validated_and_owner_scoped() {
    let url = std::env::var("TEST_DATABASE_URL").expect("disposable test database");
    let database = TestDatabase::connect(&url).await.unwrap();
    Migrator::up(&database.db, None).await.unwrap();
    let config = Config::from_lookup(|key| match key {
        "DATABASE_URL" => Some(url.clone()), "AUTH_MODE" => Some("bearer".into()),
        "AUTH_PUBLIC_KEY_PEM" => Some(include_str!("fixtures/test-only-public.pem").into()),
        "AUTH_ISSUER" => Some("test".into()), "AUTH_AUDIENCE" => Some("api".into()), _ => None,
    }).unwrap();
    let client = TestClient::new(app(AppState::new(database.db.clone() ), &config));
    client.request("GET", "/__TABLE__", None).await.assert_status(401);
    let owner = client.clone().bearer(token("owner", "__TABLE__:read __TABLE__:write"));
    let other = client.clone().bearer(token("other", "__TABLE__:read __TABLE__:write"));
    let reader = client.bearer(token("owner", "__TABLE__:read"));
    let input: Value = serde_json::to_value(fixture()).unwrap();
    reader.request("POST", "/__TABLE__", Some(input.clone())).await.assert_status(403);
    owner.request("POST", "/__TABLE__", Some(json!({}))).await.assert_status(422);
    let created = owner.request("POST", "/__TABLE__", Some(input.clone())).await;
    created.assert_status(201);
    assert!(created.body["data"].get("owner").is_none());
    let path = format!("/__TABLE__/{}", created.body["data"]["id"].as_str().unwrap());
    other.request("GET", &path, None).await.assert_status(404);
    other.request("PUT", &path, Some(input.clone())).await.assert_status(404);
    other.request("DELETE", &path, None).await.assert_status(404);
    assert_eq!(other.request("GET", "/__TABLE__", None).await.body["data"], json!([]));
    owner.request("GET", &path, None).await.assert_status(200);
    let mut updated=input.clone();
    if let Some(value)=updated.get_mut("name") { *value=json!("updated"); }
    let response=owner.request("PUT", &path, Some(updated.clone())).await;
    response.assert_status(200);
    for (key,value) in updated.as_object().unwrap() {assert_eq!(&response.body["data"][key],value);}
    let patched=owner.request("PATCH", &path, Some(json!({}))).await;
    patched.assert_status(200);
    assert_eq!(patched.body,response.body);
    owner.request("POST", "/__TABLE__", Some(input)).await.assert_status(201);
    let first=owner.request("GET", "/__TABLE__?limit=1", None).await;
    first.assert_status(200);
    let next=format!("/__TABLE__?limit=1&after={}",first.body["page"]["next_cursor"].as_str().unwrap());
    other.request("GET",&next,None).await.assert_status(422);
    let second=owner.request("GET",&next,None).await;
    second.assert_status(200);
    assert_ne!(first.body["data"][0]["id"],second.body["data"][0]["id"]);
    owner.request("GET", "/__TABLE__?filter[unknown]=x", None).await.assert_status(400);
    owner.request("DELETE", &path, None).await.assert_status(204);
    owner.request("GET", &path, None).await.assert_status(404);
    database.cleanup().await.unwrap();
}

#[tokio::test]
async fn __TABLE___operation_and_job_share_commit_and_rollback() {
    use __CRATE__::features::__TABLE__::{create_record, get_record};
    use sea_orm::{TransactionTrait, ConnectionTrait};
    let database = TestDatabase::connect(&std::env::var("TEST_DATABASE_URL").unwrap()).await.unwrap();
    Migrator::up(&database.db, None).await.unwrap();
    let principal = bracel::identity::Principal::new("test".into(), "owner".into(), "__TABLE__:read __TABLE__:write".into()).unwrap();
    let input = fixture;
    for commit in [true, false] {
        let tx = database.db.begin().await.unwrap();
        let record = create_record(&tx, &principal, input()).await.unwrap_or_else(|_| panic!("valid operation"));
        let job = bracel::jobs::enqueue(&tx, &bracel::jobs::JobSpec {
            kind: "example.ping".into(), version: 1, payload: json!({}),
            dedupe_key: record.id.to_string(), max_attempts: 3,
        }).await.unwrap();
        assert!(get_record(&database.db, &principal, record.id).await.is_err());
        if commit { tx.commit().await.unwrap(); } else {
            assert!(tx.execute_unprepared("SELECT 1/0").await.is_err());
            tx.rollback().await.unwrap();
        }
        assert_eq!(get_record(&database.db, &principal, record.id).await.is_ok(), commit);
        let found = database.db.query_one_raw(sea_orm::Statement::from_sql_and_values(sea_orm::DbBackend::Postgres,
            "SELECT id FROM bracel_jobs WHERE id=$1", [job.into()])).await.unwrap();
        assert_eq!(found.is_some(), commit);
    }
    database.cleanup().await.unwrap();
}

fn fixture() -> __CRATE__::features::__TABLE__::Write__TYPE__ {
    #[allow(unused_imports)]
    use uuid::Uuid;
    __CRATE__::features::__TABLE__::Write__TYPE__ { __FIXTURE__ }
}
