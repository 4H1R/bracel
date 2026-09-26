mod generate;
mod inspect;
mod knowledge;
mod mcp;
mod project;
mod util;

use knowledge::Knowledge;
use project::{Config, Project};
use serde_json::{Value, json};
use std::{path::Path, process::ExitCode};
use util::*;

const USAGE: &str = "bracel ai install [--agents codex,claude,cursor] [--manifest-path Cargo.toml] [--features FEATURES] [--no-default-features] [--target TRIPLE]\nbracel ai sync [--check|--dry-run]\nbracel ai info|capabilities|doctor|inspect [--json]\nbracel ai search QUERY [--limit 8]\nbracel ai bundle --output NEW_DIRECTORY\nbracel ai mcp\nAll commands accept --root DIRECTORY. Configuration: .bracel/ai.json. All CLI results are JSON.";

fn execute(
    root: &Path,
    config: Config,
    action: &str,
    query: Option<&str>,
    limit: usize,
) -> Result<Value> {
    let project = Project::resolve(root, config)?;
    let knowledge = Knowledge::load(&project)?;
    let mut freshness = generate::doctor(&project, &knowledge);
    let context_status = json!({"ok":freshness["ok"],"checks":freshness["checks"],"generation":freshness["generation"]});
    match action {
        "info" => Ok(
            json!({"schema_version":1,"ok":true,"project":project.provenance(),"freshness":context_status,"warnings":knowledge.warnings}),
        ),
        "capabilities" => Ok(
            json!({"schema_version":1,"ok":true,"capabilities":knowledge.capabilities,"freshness":context_status,"warnings":knowledge.warnings}),
        ),
        "search" => {
            let mut result = knowledge.search(query.ok_or("Missing search query")?, limit)?;
            result["freshness"] = context_status;
            result["ok"] = json!(true);
            Ok(result)
        }
        "doctor" => {
            if project.config.inspection_executable.is_some() {
                let report = inspect::inspect(&project).unwrap_or_else(
                    |error| json!({"ok":false,"compiled_state":"unknown","error":error}),
                );
                if report["ok"] != true {
                    freshness["ok"] = json!(false);
                }
                freshness["compiled_state"] = report["compiled_state"].clone();
                freshness["inspection"] = report;
            }
            Ok(freshness)
        }
        "inspect" => inspect::inspect(&project),
        _ => Err("Unknown AI action".into()),
    }
}

pub fn run(args: &[String]) -> ExitCode {
    if args.is_empty() || matches!(args[0].as_str(), "help" | "--help") {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    let action = &args[0];
    let result = (|| -> Result<Option<Value>> {
        let mut root = std::env::current_dir().map_err(|e| e.to_string())?;
        let mut manifest = None;
        let mut agents = None;
        let mut features = None;
        let mut target = None;
        let mut no_default = false;
        let mut check = false;
        let mut dry = false;
        let mut query = None;
        let mut limit = 8;
        let mut output = None;
        let mut i = 1;
        while i < args.len() {
            let arg = &args[i];
            match arg.as_str() {
                "--json" => {}
                "--check" => check = true,
                "--dry-run" => dry = true,
                "--no-default-features" => no_default = true,
                "--root" | "--manifest-path" | "--agents" | "--features" | "--target"
                | "--limit" | "--output" => {
                    i += 1;
                    let value = args
                        .get(i)
                        .ok_or_else(|| format!("Missing value for {arg}"))?;
                    match arg.as_str() {
                        "--root" => root = value.into(),
                        "--manifest-path" => manifest = Some(value.clone()),
                        "--agents" => {
                            agents = Some(value.split(',').map(String::from).collect::<Vec<_>>())
                        }
                        "--features" => {
                            features = Some(value.split(',').map(String::from).collect::<Vec<_>>())
                        }
                        "--target" => target = Some(value.clone()),
                        "--limit" => limit = value.parse().map_err(|_| "Invalid limit")?,
                        "--output" => output = Some(std::path::PathBuf::from(value)),
                        _ => unreachable!(),
                    }
                }
                _ if action == "search" && !arg.starts_with('-') && query.is_none() => {
                    query = Some(arg.clone())
                }
                _ => return Err(format!("Unknown argument: {arg}")),
            }
            i += 1;
        }
        if action != "install"
            && (manifest.is_some()
                || agents.is_some()
                || features.is_some()
                || target.is_some()
                || no_default)
        {
            return Err(
                "Resolution/agent options apply to install; later edit .bracel/ai.json".into(),
            );
        }
        if (check || dry) && action != "sync" && action != "install" {
            return Err("Check/dry-run options apply to install/sync".into());
        }
        if check && dry {
            return Err("Choose --check or --dry-run".into());
        }
        if output.is_some() && action != "bundle" {
            return Err("--output applies only to bundle".into());
        }
        root = root.canonicalize().map_err(|e| e.to_string())?;
        let mut config = Config::load(&root)?;
        if let Some(value) = manifest {
            config.manifest = value;
        }
        if let Some(mut value) = agents {
            value.sort();
            value.dedup();
            config.agents = value;
        }
        if let Some(mut value) = features {
            value.sort();
            value.dedup();
            config.features = value;
        }
        if target.is_some() {
            config.target = target;
        }
        if no_default {
            config.no_default_features = true;
        }
        if action == "mcp" {
            mcp::serve(root)?;
            return Ok(None);
        }
        let result = match action.as_str() {
            "install" | "sync" => {
                let project = Project::resolve(&root, config)?;
                let knowledge = Knowledge::load(&project)?;
                generate::sync(&project, &knowledge, check, dry)?
            }
            "bundle" => {
                let project = Project::resolve(&root, config)?;
                Knowledge::load(&project)?;
                inspect::bundle(&project, &output.ok_or("Missing --output")?)?
            }
            _ => execute(&root, config, action, query.as_deref(), limit)?,
        };
        Ok(Some(result))
    })();
    match result {
        Ok(Some(value)) => {
            println!("{}", value);
            if value.get("ok") == Some(&json!(false)) {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            }
        }
        Ok(None) => ExitCode::SUCCESS,
        Err(error) => {
            if action == "mcp" {
                eprintln!("{error}");
            } else {
                println!("{}", json!({"schema_version":1,"ok":false,"error":error}));
            }
            ExitCode::from(2)
        }
    }
}
