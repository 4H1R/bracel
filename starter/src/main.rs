use bracel_starter::tooling::{self, Command};
use bracel_starter::{AppState, app, config::Config, db, migrations::Migrator};
use sea_orm_migration::MigratorTrait;
use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    let command = match Command::parse(std::env::args().skip(1)) {
        Ok(command) => command,
        Err(usage) => {
            eprintln!("{usage}");
            return ExitCode::from(2);
        }
    };
    if let Command::Custom(name, args) = &command
        && name == "commands"
    {
        if !args.is_empty() {
            eprintln!("commands takes no arguments");
            return ExitCode::from(2);
        }
        println!(
            "{}",
            serde_json::json!({"schema_version":1, "commands":bracel_starter::commands::registry().manifest()})
        );
        return ExitCode::SUCCESS;
    }
    if command == Command::Help {
        println!("{}", tooling::USAGE);
        return ExitCode::SUCCESS;
    }
    if let Command::Diagnose {
        inspect,
        json,
        deploy,
        database,
    } = command
    {
        let report = tooling::diagnose(Config::from_env(), inspect, deploy, database).await;
        println!("{}", report.render(json));
        return if report.ok {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        };
    }
    #[cfg(feature = "telemetry")]
    let telemetry = if let Ok(endpoint) = std::env::var("OTLP_ENDPOINT") {
        match bracel_integrations::telemetry::Telemetry::otlp("bracel-starter", &endpoint, 1.0)
            .and_then(|telemetry| {
                telemetry.install_tracing()?;
                Ok(telemetry)
            }) {
            Ok(telemetry) => Some(telemetry),
            Err(_) => {
                eprintln!("Invalid telemetry configuration");
                return ExitCode::FAILURE;
            }
        }
    } else {
        None
    };
    #[cfg(feature = "telemetry")]
    let installed = telemetry.is_some();
    #[cfg(not(feature = "telemetry"))]
    let installed = false;
    if !installed {
        tracing_subscriber::fmt()
            .json()
            .with_max_level(tracing::Level::INFO)
            .with_target(false)
            .init();
    }
    let outcome = match run(command).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            tracing::error!(error = %message, "application stopped");
            ExitCode::FAILURE
        }
    };
    #[cfg(feature = "telemetry")]
    if let Some(telemetry) = telemetry {
        let _ = telemetry.shutdown();
    }
    outcome
}
async fn run(command: Command) -> Result<(), String> {
    let config = Config::from_env()?;
    if command == Command::Serve
        && let Ok(issuer) = std::env::var("AUTH_DISCOVERY_URL")
    {
        if std::env::var("AUTH_ISSUER").ok().as_deref() != Some(&issuer) {
            return Err("AUTH_DISCOVERY_URL must match the configured issuer".into());
        }
        #[cfg(feature = "identity")]
        {
            let provider = bracel_integrations::identity::IdentityProvider::connect(
                &issuer,
                config
                    .http
                    .auth
                    .clone()
                    .ok_or("Bearer authentication required")?,
            )
            .await
            .map_err(|_| "Identity discovery failed")?;
            let (stop, receiver) = tokio::sync::watch::channel(false);
            tokio::spawn(async move {
                shutdown().await;
                let _ = stop.send(true);
            });
            tokio::spawn(provider.run(receiver));
        }
        #[cfg(not(feature = "identity"))]
        return Err("Remote identity requires the identity feature".into());
    }
    let database = db::connect(&config)
        .await
        .map_err(|_| "database connection failed")?;
    if let Command::Custom(name, args) = command {
        let result = bracel_starter::commands::registry()
            .run(&name, args, database.clone())
            .await;
        database
            .close()
            .await
            .map_err(|_| "database close failed")?;
        println!(
            "{}",
            serde_json::json!({"schema_version":1,"result":result.map_err(str::to_owned)?})
        );
        return Ok(());
    }
    if command == Command::Migrate {
        Migrator::up(&database, None)
            .await
            .map_err(|_| "migration failed; inspect migration status with an administrator")?;
        database
            .close()
            .await
            .map_err(|_| "database close failed")?;
        return Ok(());
    }
    let listener = tokio::net::TcpListener::bind(config.http.bind)
        .await
        .map_err(|_| "HTTP bind failed")?;
    tracing::info!(bind = %config.http.bind, example_enabled = config.enable_example, "listening");
    let server = axum::serve(
        listener,
        app(
            AppState {
                db: database.clone(),
            },
            &config,
        )
        .into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown());
    let drain = async {
        shutdown().await;
        tokio::time::sleep(std::time::Duration::from_secs(15)).await;
    };
    tokio::select! {result=std::future::IntoFuture::into_future(server)=>{result.map_err(|_|"HTTP server failed")?;},_=drain=>{tracing::warn!("connection drain deadline reached");}}
    database
        .close()
        .await
        .map_err(|_| "database close failed")?;
    Ok(())
}
async fn shutdown() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("install Ctrl-C handler");
    };
    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("install SIGTERM handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! { _ = ctrl_c => {}, _ = terminate => {} }
    tracing::info!("shutdown requested; draining requests");
}
