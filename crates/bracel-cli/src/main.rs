use std::process::ExitCode;
mod ai;
mod generate;
mod lifecycle;

const USAGE: &str = "Usage: bracel setup [--native|--docker]\n       bracel new <directory> [--native|--docker]\n       bracel dev [--native|--docker] [--no-services] [--no-migrate]\n       bracel down\n       bracel run [--native|--docker] <application-command> [arguments]\n       bracel cargo [--native|--docker] <cargo-arguments>\n       bracel upgrade [--check] [--to VERSION] [--native|--docker]\n       bracel self update [--to VERSION]\n       bracel make resource NAME --field name:TYPE [--crud] [--dry-run] [--json]\n       bracel make job|event|policy|command|migration NAME [--dry-run] [--json]\nTypes: string, i64, bool, uuid, date, decimal, enum(a|b); append ? for nullable.\nStart with bracel setup. Docker development needs Git and Docker; native development also needs Rust and a C/C++ toolchain.";

fn main() -> ExitCode {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|arg| arg == "ai") {
        if let Some(result) = lifecycle::ai(&args[1..]) {
            return result;
        }
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
    lifecycle::run(&args)
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
