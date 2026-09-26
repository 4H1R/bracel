use std::{
    path::Path,
    process::{Command, ExitCode},
};
mod ai;
mod generate;

const USAGE: &str = "Usage: bracel new <directory>\n       bracel make resource NAME --field name:TYPE [--crud] [--dry-run] [--json]\n       bracel make job|event|policy|command|migration NAME [--dry-run] [--json]\nTypes: string, i64, bool, uuid, date, decimal, enum(a|b); append ? for nullable.\nNew applications require Git and access to github.com/4H1R/bracel-starter.";
const STARTER_TAG: &str = concat!("v", env!("CARGO_PKG_VERSION"));

fn main() -> ExitCode {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|arg| arg == "ai") {
        return ai::run(&args[1..]);
    }
    if args.first().is_some_and(|arg| arg == "make") {
        return generate_resource(&args[1..]);
    }
    if args == ["--help"] || args == ["help"] {
        println!("{USAGE}");
        println!(
            "       bracel ai <install|sync|info|search|capabilities|doctor|inspect|bundle|mcp> (ai --help for options)"
        );
        return ExitCode::SUCCESS;
    }
    if args == ["--version"] {
        println!("bracel {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }
    if args.len() != 2 || args[0] != "new" || args[1].is_empty() {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    }
    let destination = Path::new(&args[1]);
    if destination.exists() {
        eprintln!("Destination already exists; choose a new directory.");
        return ExitCode::FAILURE;
    }
    let destination = match std::path::absolute(destination) {
        Ok(path) => path,
        Err(_) => {
            eprintln!("Cannot resolve destination.");
            return ExitCode::FAILURE;
        }
    };
    let status = Command::new("git")
        .args([
            "-c",
            "advice.detachedHead=false",
            "clone",
            "--depth",
            "1",
            "--origin",
            "starter",
            "--branch",
            STARTER_TAG,
            "--",
            "https://github.com/4H1R/bracel-starter.git",
        ])
        .arg(&destination)
        .status();
    if !status.is_ok_and(|status| status.success()) {
        eprintln!("Starter clone failed. Check Git installation and GitHub access.");
        return ExitCode::FAILURE;
    }
    if !Command::new("git")
        .arg("-C")
        .arg(&destination)
        .args(["switch", "-c", "main"])
        .status()
        .is_ok_and(|status| status.success())
    {
        eprintln!(
            "Starter cloned, but creating the main branch failed. The directory was retained."
        );
        return ExitCode::FAILURE;
    }
    println!(
        "Created Bracel application. Follow README.md in the new directory.\nThe starter remote and shallow history are retained; add your own origin."
    );
    ExitCode::SUCCESS
}

fn generate_resource(args: &[String]) -> ExitCode {
    let run = || -> Result<(), String> {
        if args.len() < 2 {
            return Err("Usage: bracel make resource Name --field name:string [--field active:bool] [--crud] [--dry-run] [--json]".into());
        }
        let mut fields = Vec::new();
        let mut dry = false;
        let mut json = false;
        let mut index = 2;
        while index < args.len() {
            match args[index].as_str() {
                "--field" => {
                    index += 1;
                    fields.push(args.get(index).ok_or("Missing field declaration")?.clone());
                }
                "--dry-run" => dry = true,
                "--json" => json = true,
                "--crud" => {}
                _ => return Err("Unknown generator option".into()),
            }
            index += 1;
        }
        let root = std::env::current_dir().map_err(|_| "Application directory unavailable")?;
        let plan = if args[0] == "resource" {
            generate::resource(&root, &args[1], &fields)?
        } else {
            if !fields.is_empty() {
                return Err("Fields apply only to resources".into());
            }
            generate::extension(&root, &args[0], &args[1])?
        };
        if !dry {
            plan.apply(&root)?;
        }
        if json {
            println!(
                "{}",
                serde_json::to_string_pretty(&plan)
                    .map_err(|_| "Cannot serialize generation plan")?
            );
        } else {
            for change in plan.changes {
                println!("{} {}", if dry { "planned" } else { "wrote" }, change.path);
            }
        }
        Ok(())
    };
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            if args.iter().any(|arg| arg == "--json") {
                eprintln!(
                    "{}",
                    serde_json::json!({"schema_version":1,"ok":false,"error":{"code":"generation_failed","message":error}})
                );
            } else {
                eprintln!("{error}");
            }
            ExitCode::from(2)
        }
    }
}
