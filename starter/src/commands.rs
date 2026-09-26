use bracel::{
    commands::{CommandInfo, Commands},
    jobs,
};
use sea_orm::DatabaseConnection;
use serde_json::json;
pub fn registry() -> Commands<DatabaseConnection> {
    let mut commands = Commands::default();
    let mut add = |name, summary, arguments, handler| {
        commands
            .register(
                CommandInfo {
                    name,
                    summary,
                    arguments,
                },
                handler,
            )
            .expect("unique commands")
    };
    // Explicit function pointers keep all command handlers on one reusable interface.
    type Handler = fn(
        DatabaseConnection,
        Vec<String>,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<serde_json::Value, &'static str>> + Send>,
    >;
    let failed: Handler = |db, _| {
        Box::pin(async move {
            Ok(json!(
                jobs::failed(&db)
                    .await
                    .map_err(|_| "Job inspection failed")?
            ))
        })
    };
    add(
        "jobs:failed",
        "List failed jobs without their payloads",
        &[],
        failed,
    );
    let retry: Handler = |db, args| {
        Box::pin(async move {
            let id = args[0].parse().map_err(|_| "Job ID must be a UUID")?;
            Ok(
                json!({"replayed":jobs::replay(&db,id,&args[1]).await.map_err(|_|"Job replay failed")?}),
            )
        })
    };
    add(
        "jobs:retry",
        "Replay one failed job and record a reason",
        &["id", "reason"],
        retry,
    );
    let tick: Handler = |db, _| {
        Box::pin(async move {
            Ok(json!({"scheduled":jobs::tick_schedules(&db).await.map_err(|_|"Scheduler failed")?}))
        })
    };
    add("schedule:tick", "Enqueue due schedules once", &[], tick);
    let scheduler: Handler = |db, _| {
        Box::pin(async move {
            let (stop, receiver) = tokio::sync::watch::channel(false);
            tokio::spawn(async move {
                shutdown().await;
                let _ = stop.send(true);
            });
            #[cfg(feature = "batteries")]
            if std::env::var("ENABLE_BATTERIES").as_deref() == Ok("true") {
                let mut receiver = receiver;
                while !*receiver.borrow() {
                    jobs::tick_schedules(&db)
                        .await
                        .map_err(|_| "Scheduler failed")?;
                    jobs::tick_calendar(&db)
                        .await
                        .map_err(|_| "Calendar scheduler failed")?;
                    tokio::select! {_=tokio::time::sleep(std::time::Duration::from_secs(1))=>{},_=receiver.changed()=>break}
                }
                return Ok(json!({"stopped":true}));
            }
            jobs::run_schedules(&db, receiver)
                .await
                .map_err(|_| "Scheduler failed")?;
            Ok(json!({"stopped":true}))
        })
    };
    add(
        "schedule:work",
        "Poll due schedules until Ctrl-C or SIGTERM",
        &[],
        scheduler,
    );
    let once: Handler = |db, _| {
        Box::pin(async move {
            Ok(
                json!({"processed":worker().tick(&db,std::time::Duration::from_secs(25)).await.map_err(|_|"Worker failed")?}),
            )
        })
    };
    add("jobs:once", "Process one available job", &[], once);
    let work: Handler = |db, _| {
        Box::pin(async move {
            let (stop, receiver) = tokio::sync::watch::channel(false);
            tokio::spawn(async move {
                shutdown().await;
                let _ = stop.send(true);
            });
            worker()
                .run(&db, receiver)
                .await
                .map_err(|_| "Worker failed")?;
            Ok(json!({"stopped":true}))
        })
    };
    add(
        "jobs:work",
        "Run the worker until Ctrl-C or SIGTERM",
        &[],
        work,
    );
    let seed: Handler = |db, _| {
        Box::pin(async move {
            jobs::enqueue(
                &db,
                &jobs::JobSpec {
                    kind: "example.ping".into(),
                    version: 1,
                    payload: json!({}),
                    dedupe_key: "example.seed".into(),
                    max_attempts: 3,
                },
            )
            .await
            .map_err(|_| "Seed failed")?;
            Ok(json!({"seeded":true}))
        })
    };
    add(
        "db:seed",
        "Idempotently seed an example queue job",
        &[],
        seed,
    );
    let revoke: Handler = |db, args| {
        Box::pin(async move {
            Ok(
                json!({"revoked":bracel::tokens::revoke(&db,args[0].parse().map_err(|_|"Token ID must be a UUID")?).await.map_err(|_|"Token revocation failed")?}),
            )
        })
    };
    add("tokens:revoke", "Revoke one machine token", &["id"], revoke);
    let issue: Handler = |db, args| {
        Box::pin(async move {
            let principal = bracel::identity::Principal::new(
                args[0].clone(),
                args[1].clone(),
                args[2].clone(),
            )?;
            let issued = bracel::tokens::issue(
                &db,
                &principal,
                args[3].parse().map_err(|_| "Lifetime must be seconds")?,
            )
            .await
            .map_err(|_| "Token issuance failed")?;
            Ok(json!({"id":issued.id,"token":issued.secret}))
        })
    };
    add(
        "tokens:issue",
        "Issue a machine token; stdout contains its secret once",
        &["issuer", "subject", "scopes", "lifetime_seconds"],
        issue,
    );
    let account_mail: Handler = |db, _| {
        Box::pin(async move {
            Ok(json!({"processed":crate::features::accounts::mail_once(&db).await?}))
        })
    };
    add(
        "auth:mail-once",
        "Process one pending password reset email",
        &[],
        account_mail,
    );
    let account_work: Handler = |db, _| {
        Box::pin(async move {
            let shutdown = shutdown();
            tokio::pin!(shutdown);
            loop {
                crate::features::accounts::mail_once(&db).await?;
                tokio::select! { _=&mut shutdown => break, _=tokio::time::sleep(std::time::Duration::from_secs(1))=>{} }
            }
            Ok(json!({"stopped":true}))
        })
    };
    add(
        "auth:mail-work",
        "Deliver password reset emails until shutdown",
        &[],
        account_work,
    );
    let account_cleanup: Handler = |db, _| {
        Box::pin(async move {
            crate::features::accounts::cleanup(&db).await?;
            Ok(json!({"cleaned":true}))
        })
    };
    add(
        "auth:cleanup",
        "Remove up to 1000 expired rows per account table",
        &[],
        account_cleanup,
    );
    #[cfg(feature = "batteries")]
    crate::batteries_commands::register(&mut commands);
    crate::extensions::commands(&mut commands);
    commands
}
pub fn worker() -> jobs::Worker {
    let mut worker = jobs::Worker::default();
    crate::extensions::jobs(&mut worker);
    worker
        .register("example.ping", 1, |payload| async move {
            if payload != json!({}) {
                return Err(jobs::Failure::Permanent("invalid_payload"));
            }
            Ok(())
        })
        .expect("valid handler");
    worker
}
pub(crate) async fn shutdown() {
    #[cfg(unix)]
    {
        let mut signal = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("SIGTERM");
        tokio::select! { _=tokio::signal::ctrl_c()=>{}, _=signal.recv()=>{} }
    }
    #[cfg(not(unix))]
    let _ = tokio::signal::ctrl_c().await;
}
