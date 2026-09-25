//! Read-only developer diagnostics. JSON stdout is a versioned public CLI contract.
use crate::{ApiDoc, config::Config, db, migrations::Migrator};
use sea_orm::{AccessMode, ConnectionTrait, DatabaseConnection, IsolationLevel, TransactionTrait};
use sea_orm_migration::{MigrationStatus, MigratorTrait};
use serde::Serialize;
use serde_json::{Value, json};
use std::time::Duration;
use utoipa::OpenApi;

pub const USAGE: &str = "Usage: bracel-starter [serve|migrate|doctor [--json] [--deploy] [--database]|inspect [--json] [--database]|--help]";

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Serve,
    Migrate,
    Help,
    Diagnose {
        inspect: bool,
        json: bool,
        deploy: bool,
        database: bool,
    },
}

impl Command {
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Self, &'static str> {
        let mut args = args.into_iter();
        let command = args.next().unwrap_or_else(|| "serve".into());
        if matches!(command.as_str(), "doctor" | "inspect") {
            let inspect = command == "inspect";
            let (mut json, mut deploy, mut database) = (false, false, false);
            for arg in args {
                match arg.as_str() {
                    "--json" if !json => json = true,
                    "--deploy" if !inspect && !deploy => deploy = true,
                    "--database" if !database => database = true,
                    _ => return Err(USAGE),
                }
            }
            return Ok(Self::Diagnose {
                inspect,
                json,
                deploy,
                database,
            });
        }
        if args.next().is_some() {
            return Err(USAGE);
        }
        match command.as_str() {
            "serve" => Ok(Self::Serve),
            "migrate" => Ok(Self::Migrate),
            "--help" | "help" | "-h" => Ok(Self::Help),
            _ => Err(USAGE),
        }
    }
}

#[derive(Serialize)]
pub struct Check {
    pub code: &'static str,
    pub severity: &'static str,
    pub message: String,
    pub remediation: &'static str,
}

#[derive(Serialize)]
pub struct Migration {
    pub name: String,
    pub status: &'static str,
}

#[derive(Serialize)]
pub struct Report {
    pub schema_version: u32,
    pub command: &'static str,
    pub ok: bool,
    pub checks: Vec<Check>,
    pub database_status: &'static str,
    pub migrations: Vec<Migration>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub application: Option<Value>,
}

impl Report {
    fn check(
        &mut self,
        code: &'static str,
        severity: &'static str,
        message: impl Into<String>,
        remediation: &'static str,
    ) {
        self.ok &= severity != "error";
        self.checks.push(Check {
            code,
            severity,
            message: message.into(),
            remediation,
        });
    }

    pub fn render(&self, as_json: bool) -> String {
        if as_json || self.command == "inspect" {
            return serde_json::to_string_pretty(self).expect("report serialization");
        }
        let mut text = format!(
            "doctor: {}
",
            if self.ok {
                "passed requested checks"
            } else {
                "failed"
            }
        );
        for check in &self.checks {
            text.push_str(&format!(
                "[{}] {}: {}
",
                check.severity, check.code, check.message
            ));
            if !check.remediation.is_empty() {
                text.push_str(&format!(
                    "  {}
",
                    check.remediation
                ));
            }
        }
        for migration in &self.migrations {
            text.push_str(&format!(
                "{} {}
",
                migration.status, migration.name
            ));
        }
        text
    }
}

fn inventory(config: Option<&Config>) -> Value {
    let api = serde_json::to_value(ApiDoc::openapi()).expect("OpenAPI serialization");
    let mut routes = Vec::new();
    for (path, item) in api["paths"].as_object().expect("OpenAPI paths") {
        for method in [
            "get", "post", "put", "patch", "delete", "head", "options", "trace",
        ] {
            if item.get(method).is_some() {
                // The teaching routes share the ENABLE_EXAMPLE gate in app().
                let enabled =
                    config.map(|config| !path.starts_with("/example/") || config.enable_example);
                let example = path.starts_with("/example/");
                routes.push(json!({ "method": method.to_ascii_uppercase(), "path": path, "enabled": enabled,
                    "authentication": config.map(|c| if example && c.auth.is_some() { "bearer" } else { "public" }),
                    "required_scope": if example && config.is_some_and(|c| c.auth.is_some()) { item[method]["x-required-scope"].clone() } else { Value::Null },
                    "rate_policy": if example { item[method]["x-rate-policy"].clone() } else { json!("exempt") },
                    "query_parameters": item[method]["parameters"].as_array().map(|p| p.iter().filter(|p| p["in"] == "query").cloned().collect::<Vec<_>>()).unwrap_or_default()
                }));
            }
        }
    }
    json!({
        "name": env!("CARGO_PKG_NAME"),
        "version": env!("CARGO_PKG_VERSION"),
        "minimum_rust_version": env!("CARGO_PKG_RUST_VERSION"),
        "locked_packages": serde_json::from_str::<Value>(include_str!(concat!(env!("OUT_DIR"), "/versions.json"))).expect("build metadata"),
        "commands": ["serve", "migrate", "doctor [--json] [--deploy] [--database]", "inspect [--json] [--database]"],
        "configuration_valid": config.is_some(),
        "example_enabled": config.map(|config| config.enable_example),
        "routes": routes,
        "middleware": config.map(|c| json!({"rate_backend":"process_local","anonymous_per_minute":c.anonymous_per_minute,"authenticated_per_minute":c.authenticated_per_minute,"writes_per_minute":c.writes_per_minute,"max_keys_per_policy":c.rate_max_keys,"max_in_flight":c.max_in_flight,"cors_configured":!c.cors_origins.is_empty()})),
        "route_scope": "Documented explicit operations; Axum implicit HEAD and fallbacks are not enumerated. Enabled means configured, not reachable or healthy.",
        "capabilities": {"http": "implemented", "postgresql": "implemented", "diagnostics": "implemented", "cursor_pagination": "implemented", "query_filters": "implemented", "middleware": "implemented", "rate_limiting": "local", "success_envelopes": "implemented", "identity": "bearer_jwt", "jobs": "recipe_only", "email": "recipe_only"},
        "documentation": {"catalog": "docs/features/index.md", "architecture": "docs/architecture.md", "tooling": "docs/features/tooling.md", "http": "docs/http.md", "database": "docs/database.md", "operations": "docs/operations.md"}
    })
}

/// Offline checks never connect to a database or print configuration values.
pub fn offline(config: Result<&Config, &str>, inspect: bool, deploy: bool) -> Report {
    let mut report = Report {
        schema_version: 1,
        command: if inspect { "inspect" } else { "doctor" },
        ok: true,
        checks: Vec::new(),
        database_status: "not_checked",
        migrations: Migrator::migrations()
            .iter()
            .map(|migration| Migration {
                name: migration.name().into(),
                status: "not_checked",
            })
            .collect(),
        application: inspect.then(|| inventory(config.as_ref().ok().copied())),
    };
    match config {
        Ok(config) => {
            report.check("CONFIG.VALID", "info", "Configuration is valid.", "");
            if deploy && config.enable_example {
                report.check(
                    "DEPLOY.EXAMPLE_ENABLED",
                    "error",
                    "Teaching routes are enabled.",
                    "Set ENABLE_EXAMPLE=false for deployment.",
                );
            } else if config.enable_example {
                report.check(
                    "CONFIG.EXAMPLE_ENABLED",
                    "warning",
                    "Teaching routes are enabled; authentication follows AUTH_MODE.",
                    "Use only for local teaching; disable before deployment.",
                );
            }
        }
        Err(error) => {
            // Do not trust caller-supplied error strings: they may contain secrets.
            let field = [
                "DATABASE_URL",
                "BIND_ADDR",
                "ENABLE_EXAMPLE",
                "DB_MAX_CONNECTIONS",
                "REQUEST_TIMEOUT_MS",
                "BODY_LIMIT_BYTES",
            ]
            .into_iter()
            .find(|field| error.starts_with(field));
            let message = field.map_or_else(
                || "Configuration is missing or invalid.".into(),
                |field| format!("{field} is missing or invalid."),
            );
            report.check(
                "CONFIG.INVALID",
                "error",
                message,
                "Check the named setting against .env.example and docs/operations.md.",
            );
        }
    }
    if deploy {
        report.check("DEPLOY.EXTERNAL_REVIEW", "warning", "Proxy/TLS, database privileges, secrets, capacity and backup recovery require external verification.", "Review docs/operations.md; this command does not certify deployment safety.");
    }
    report
}

/// Use a read-only transaction as well as SeaORM's non-installing status API.
pub async fn migration_status(
    database: &DatabaseConnection,
) -> Result<Vec<Migration>, sea_orm::DbErr> {
    let transaction = database
        .begin_with_config(
            Some(IsolationLevel::RepeatableRead),
            Some(AccessMode::ReadOnly),
        )
        .await?;
    transaction
        .execute_unprepared("SET LOCAL statement_timeout = '2000ms'")
        .await?;
    let migrations = Migrator::get_migration_with_status_read_only(&transaction).await?;
    transaction.rollback().await?;
    Ok(migrations
        .into_iter()
        .map(|migration| Migration {
            name: migration.name().into(),
            status: match migration.status() {
                MigrationStatus::Applied => "applied",
                MigrationStatus::Pending => "pending",
            },
        })
        .collect())
}

pub async fn diagnose(
    config: Result<Config, String>,
    inspect: bool,
    deploy: bool,
    check_database: bool,
) -> Report {
    let mut report = offline(config.as_ref().map_err(String::as_str), inspect, deploy);
    if !check_database {
        report.check(
            "DATABASE.NOT_CHECKED",
            "info",
            "Database connectivity and migration history were not checked.",
            "Pass --database to perform bounded read-only checks.",
        );
        return report;
    }
    let Ok(mut config) = config else {
        report.check(
            "DATABASE.SKIPPED",
            "info",
            "Database check requires valid configuration.",
            "Correct configuration and retry --database.",
        );
        return report;
    };
    config.db_max_connections = 1;
    let connected = tokio::time::timeout(Duration::from_secs(5), db::connect(&config)).await;
    let Ok(Ok(database)) = connected else {
        report.database_status = "unavailable";
        report.check(
            "DATABASE.UNAVAILABLE",
            "error",
            "Database connection failed or timed out.",
            "Check database availability, credentials, network and TLS configuration.",
        );
        return report;
    };
    let status = tokio::time::timeout(Duration::from_secs(5), migration_status(&database)).await;
    // A timed-out query may still be rolling back; bound pool shutdown too.
    let _ = tokio::time::timeout(Duration::from_secs(2), database.close()).await;
    match status {
        Ok(Ok(migrations)) => {
            report.database_status = "checked";
            let pending = migrations
                .iter()
                .filter(|migration| migration.status == "pending")
                .count();
            report.migrations = migrations;
            if pending > 0 {
                report.check("DATABASE.PENDING_MIGRATIONS", if inspect { "warning" } else { "error" }, format!("{pending} migration(s) pending."), "Review the migration and run the explicit migrate command with the deployment migration role.");
            } else {
                report.check(
                    "DATABASE.MIGRATIONS_CURRENT",
                    "info",
                    "Migration history matches this binary.",
                    "History is not a schema-drift or application-health check.",
                );
            }
        }
        _ => {
            report.database_status = "unavailable";
            report.check("DATABASE.INSPECTION_FAILED", "error", "Migration inspection failed or timed out.", "Check migration-history read permissions, locks and compatibility with this binary; see docs/operations.md.");
        }
    }
    report
}
