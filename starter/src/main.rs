use bracel_starter::cli::tooling::{self, Command};
use bracel_starter::{AppState, bootstrap, config::Config, db, migrations::Migrator};
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
            serde_json::json!({"schema_version":1, "commands":bracel_starter::cli::commands::registry().manifest()})
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
    let config = match Config::from_env() {
        Ok(config) => config,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::FAILURE;
        }
    };
    #[cfg(feature = "telemetry")]
    let telemetry = if let Some(endpoint) = &config.providers.telemetry_endpoint {
        match bracel_integrations::telemetry::Telemetry::otlp("bracel-starter", endpoint, 1.0)
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
    let outcome = match run(command, config).await {
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
async fn run(command: Command, config: Config) -> Result<(), String> {
    let database = db::connect(&config)
        .await
        .map_err(|_| "database connection failed")?;
    if let Command::Custom(name, args) = command {
        let result =
            bracel_starter::cli::commands::registry_with_providers(config.providers.clone())
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
    let state = AppState::from_config(database.clone(), &config);
    let resources = bootstrap::Resources::build(&state, &config)?;
    let router = bracel_starter::http::router(state, &config, &resources);
    let mut lifecycle = bootstrap::Lifecycle::default();
    #[cfg(feature = "identity")]
    if let Some(issuer) = &config.providers.discovery_url {
        let provider = bracel_integrations::identity::IdentityProvider::connect(
            issuer,
            config
                .http
                .auth
                .clone()
                .ok_or("Bearer authentication required")?,
        )
        .await
        .map_err(|_| "Identity discovery failed")?;
        lifecycle.spawn(provider.run(lifecycle.subscribe()));
    }
    lifecycle.listen();
    let stop = lifecycle.subscribe();
    let stream_resources = resources.clone();
    lifecycle.spawn(async move {
        bootstrap::stopped(stop).await;
        stream_resources.shutdown();
    });
    tracing::info!(bind = %config.http.bind, example_enabled = config.enable_example, "listening");
    let server = axum::serve(
        listener,
        router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(bootstrap::stopped(lifecycle.subscribe()));
    let stop = lifecycle.subscribe();
    let drain = async {
        bootstrap::stopped(stop).await;
        tokio::time::sleep(std::time::Duration::from_secs(15)).await;
    };
    let result = tokio::select! {
        result=std::future::IntoFuture::into_future(server)=>result.map_err(|_| "HTTP server failed"),
        _=drain=>{tracing::warn!("connection drain deadline reached"); Ok(())}
    };
    resources.shutdown();
    lifecycle.finish().await;
    result?;
    database
        .close()
        .await
        .map_err(|_| "database close failed")?;
    Ok(())
}
