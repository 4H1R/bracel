use std::{
    path::Path,
    process::{Command, ExitCode},
};

const USAGE: &str =
    "Usage: bracel new <directory>\nRequires Git and access to github.com/4H1R/bracel-starter.";

fn main() -> ExitCode {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args == ["--help"] || args == ["help"] {
        println!("{USAGE}");
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
            "clone",
            "--depth",
            "1",
            "--origin",
            "starter",
            "--branch",
            "v0.1.0",
            "--",
            "https://github.com/4H1R/bracel-starter.git",
        ])
        .arg(&destination)
        .status();
    if !status.is_ok_and(|status| status.success()) {
        eprintln!("Starter clone failed. Check Git installation and GitHub access.");
        return ExitCode::FAILURE;
    }
    println!(
        "Created Bracel application. Follow README.md in the new directory.\nThe starter remote and shallow history are retained; add your own origin."
    );
    ExitCode::SUCCESS
}
