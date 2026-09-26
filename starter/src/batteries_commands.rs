use bracel::{
    commands::{CommandInfo, Commands},
    jobs,
};
use sea_orm::DatabaseConnection;
use serde_json::json;
pub fn register(commands: &mut Commands<DatabaseConnection>) {
    commands.register(CommandInfo{name:"tenants:grant",summary:"Grant a tenant role through the trusted operator CLI",arguments:&["tenant","issuer","subject","role"]},|db:DatabaseConnection,args:Vec<String>|async move {
        use sea_orm::ConnectionTrait;
        let id=args[0].parse::<uuid::Uuid>().map_err(|_|"Invalid tenant ID")?;
        if !matches!(args[3].as_str(),"reader"|"writer"|"admin") {return Err("Invalid tenant role");}
        let principal=bracel::identity::Principal::new(args[1].clone(),args[2].clone(),String::new())?;
        db.execute_raw(bracel_data::sql("INSERT INTO bracel_memberships(tenant,principal,role) VALUES($1,$2,$3) ON CONFLICT(tenant,principal) DO UPDATE SET role=excluded.role",vec![id.into(),principal.cursor_scope().into(),args[3].clone().into()])).await.map_err(|_|"Membership update failed")?;
        Ok(json!({"granted":true}))
    }).expect("unique command");
    commands.register(CommandInfo{name:"delivery:once",summary:"Process one configured mail or webhook delivery",arguments:&["queue"]},|db:DatabaseConnection,args:Vec<String>|async move{
        let mut worker=jobs::Worker::default();
        match args[0].as_str(){
            "mail"=>{
                let port=std::env::var("MAIL_LOCAL_PORT").map_err(|_|"MAIL_LOCAL_PORT required for local SMTP")?.parse().map_err(|_|"Invalid SMTP port")?;
                bracel_delivery::register_mail(&mut worker,db.clone(),bracel_integrations::mail::Mailer::local(port))?;
            },
            "webhooks"=>{
                let url=std::env::var("WEBHOOK_URL").map_err(|_|"WEBHOOK_URL required")?;
                let secret=std::env::var("WEBHOOK_SECRET").map_err(|_|"WEBHOOK_SECRET required")?;
                let origin=url::Url::parse(&url).map_err(|_|"Invalid webhook URL")?.origin().ascii_serialization();
                let client=bracel_integrations::http::Outbound::new(&[&origin],65536,std::time::Duration::from_secs(5)).map_err(|_|"Invalid webhook destination")?;
                bracel_delivery::webhook::register(&mut worker,std::collections::BTreeMap::from([("primary".into(),bracel_delivery::webhook::Endpoint{url,secret:secret.into_bytes()})]),client)?;
            },
            "incoming"=>{
                let database=db.clone();
                worker.register_typed(move|job:bracel_delivery::IncomingJob|{
                    let db=database.clone();
                    async move {
                        if let Some(receipt)=bracel_delivery::Incoming::begin(&db,&job).await.map_err(|_|jobs::Failure::Retry("inbox_unavailable"))? {
                            receipt.complete().await.map_err(|_|jobs::Failure::Retry("inbox_unavailable"))?;
                        }
                        Ok(())
                    }
                })?;
            },
            _=>return Err("Queue must be mail, webhooks or incoming"),
        }
        Ok(json!({"processed":worker.tick_queue(&db,&args[0],std::time::Duration::from_secs(15)).await.map_err(|_|"Delivery worker failed")?}))
    }).expect("unique command");
    commands
        .register(
            CommandInfo {
                name: "jobs:parallel",
                summary: "Run a named application queue with bounded concurrency and deadlines",
                arguments: &["queue", "concurrency", "timeout_seconds"],
            },
            |db: DatabaseConnection, args: Vec<String>| async move {
                let concurrency = args[1].parse().map_err(|_| "Invalid concurrency")?;
                let seconds = args[2].parse::<u64>().map_err(|_| "Invalid deadline")?;
                if !(1..=3590).contains(&seconds) {
                    return Err("Deadline must be 1..3590 seconds");
                }
                let (stop, receiver) = tokio::sync::watch::channel(false);
                tokio::spawn(async move {
                    crate::commands::shutdown().await;
                    let _ = stop.send(true);
                });
                std::sync::Arc::new(crate::commands::worker())
                    .run_parallel(
                        db,
                        args[0].clone(),
                        concurrency,
                        std::time::Duration::from_secs(seconds),
                        receiver,
                    )
                    .await
                    .map_err(|_| "Parallel worker failed")?;
                Ok(json!({"stopped":true}))
            },
        )
        .expect("unique command");
    commands.register(CommandInfo{name:"events:prune",summary:"Prune at most 1000 retained positions through an explicit cursor position",arguments:&["through"]},|db:DatabaseConnection,args:Vec<String>|async move {
        let store=bracel_realtime::EventStore::new(db,1).map_err(|_|"Invalid event store")?;
        Ok(json!({"deleted":store.prune(args[0].parse().map_err(|_|"Invalid position")?).await.map_err(|_|"Event retention failed")?}))
    }).expect("unique command");
    commands.register(CommandInfo{name:"idempotency:cleanup",summary:"Remove up to 500 expired replay records",arguments:&[]},|db:DatabaseConnection,_|async move {
        Ok(json!({"deleted":bracel_data::cleanup_idempotency(&db).await.map_err(|_|"Replay cleanup failed")?}))
    }).expect("unique command");
    commands
        .register(
            CommandInfo {
                name: "files:cleanup",
                summary: "Reconcile up to 20 expired or deleted uploads",
                arguments: &[],
            },
            |db: DatabaseConnection, _| async move {
                let root = std::env::var("FILES_ROOT").map_err(|_| "FILES_ROOT required")?;
                let storage = bracel_integrations::storage::Storage::local(root, 1024 * 1024)
                    .map_err(|_| "Invalid files storage")?;
                let files = bracel_files::Files::new(db, storage, 1024 * 1024)
                    .map_err(|_| "Invalid files configuration")?;
                Ok(json!({"deleted":files.cleanup().await.map_err(|_|"File cleanup failed")?}))
            },
        )
        .expect("unique command");
    commands
        .register(
            CommandInfo {
                name: "schedule:calendar",
                summary: "Install a calendar ping schedule with overlap prevention",
                arguments: &["name", "expression", "timezone"],
            },
            |db: DatabaseConnection, args: Vec<String>| async move {
                jobs::calendar(
                    &db,
                    &args[0],
                    &args[1],
                    &args[2],
                    &jobs::JobSpec {
                        kind: "example.ping".into(),
                        version: 1,
                        payload: json!({}),
                        dedupe_key: format!("calendar:{}", args[0]),
                        max_attempts: 3,
                    },
                )
                .await
                .map_err(|_| "Invalid calendar schedule")?;
                Ok(json!({"scheduled":true}))
            },
        )
        .expect("unique command");
    commands.register(CommandInfo{name:"schedule:calendar-tick",summary:"Coalesce due calendar schedules once",arguments:&[]},|db:DatabaseConnection,_|async move{
        Ok(json!({"due":jobs::tick_calendar(&db).await.map_err(|_|"Calendar tick failed")?}))
    }).expect("unique command");
}
