use bracel::{
    commands::{CommandInfo, Commands},
    jobs,
};
use sea_orm::DatabaseConnection;
use serde_json::json;
pub fn registry() -> Commands<DatabaseConnection> {
    registry_with_providers(crate::provider_settings::Providers::default())
}
pub fn registry_with_providers(
    providers: crate::provider_settings::Providers,
) -> Commands<DatabaseConnection> {
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
            Ok(
                json!({"scheduled":crate::schedules::tick(&db).await.map_err(|_|"Scheduler failed")?}),
            )
        })
    };
    add("schedule:tick", "Enqueue due schedules once", &[], tick);
    let scheduler: Handler = |db, _| {
        Box::pin(async move {
            let mut lifecycle = crate::bootstrap::Lifecycle::default();
            lifecycle.listen();
            let receiver = lifecycle.subscribe();
            crate::schedules::run(&db, receiver)
                .await
                .map_err(|_| "Scheduler failed")?;
            lifecycle.finish().await;
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
                json!({"processed":crate::jobs::worker(db.clone()).tick(&db,std::time::Duration::from_secs(25)).await.map_err(|_|"Worker failed")?}),
            )
        })
    };
    add("jobs:once", "Process one available job", &[], once);
    let work: Handler = |db, _| {
        Box::pin(async move {
            let mut lifecycle = crate::bootstrap::Lifecycle::default();
            lifecycle.listen();
            let receiver = lifecycle.subscribe();
            crate::jobs::worker(db.clone())
                .run(&db, receiver)
                .await
                .map_err(|_| "Worker failed")?;
            lifecycle.finish().await;
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
    let sync: Handler = |db, _| {
        Box::pin(async move {
            crate::schedules::sync(&db)
                .await
                .map_err(|_| "Schedule registration failed")?;
            Ok(json!({"synced":true}))
        })
    };
    add(
        "schedule:sync",
        "Install or update definitions from src/schedules.rs",
        &[],
        sync,
    );
    for (name, summary, continuous) in [
        ("auth:mail-once", "Process one pending account email", false),
        (
            "auth:mail-work",
            "Deliver account emails until shutdown",
            true,
        ),
    ] {
        let settings = providers.mail.clone();
        commands.register(CommandInfo {name, summary, arguments: &[]}, move |db: DatabaseConnection, _| {
            let settings = settings.clone();
            async move {
                let worker = crate::features::accounts::MailWorker::from_settings(&settings)?;
                if !continuous { return Ok(json!({"processed":worker.tick(&db).await?})); }
                let shutdown = crate::bootstrap::signal();
                tokio::pin!(shutdown);
                loop {
                    worker.tick(&db).await?;
                    tokio::select! { _ = &mut shutdown => break, _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => {} }
                }
                Ok(json!({"stopped":true}))
            }
        }).expect("unique command");
    }
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
                let mut lifecycle = crate::bootstrap::Lifecycle::default();
                lifecycle.listen();
                let receiver = lifecycle.subscribe();
                std::sync::Arc::new(crate::jobs::worker(db.clone()))
                    .run_parallel(
                        db,
                        args[0].clone(),
                        concurrency,
                        std::time::Duration::from_secs(seconds),
                        receiver,
                    )
                    .await
                    .map_err(|_| "Parallel worker failed")?;
                lifecycle.finish().await;
                Ok(json!({"stopped":true}))
            },
        )
        .expect("unique command");
    #[cfg(feature = "batteries")]
    crate::batteries::commands::register(&mut commands, providers);
    crate::extensions::commands(&mut commands);
    commands
}
