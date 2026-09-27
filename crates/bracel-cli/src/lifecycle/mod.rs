mod project;
mod release;
mod upgrade;

use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, ExitCode, Stdio},
};

type Result<T> = std::result::Result<T, String>;
const FRAMEWORK: &str = "https://github.com/4H1R/bracel";
const STARTER: &str = "https://github.com/4H1R/bracel-starter";

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Docker,
    Native,
}

fn status(command: &mut Command, description: &str) -> Result<()> {
    let result = command
        .status()
        .map_err(|e| format!("{description}: {e}"))?;
    if !result.success() {
        return Err(format!("{description} failed ({result})."));
    }
    Ok(())
}

fn output(command: &mut Command, description: &str) -> Result<String> {
    let result = command
        .output()
        .map_err(|e| format!("{description}: {e}"))?;
    if !result.status.success() {
        // Git/Cargo can include credentials in diagnostics; these lookups need only a summary.
        return Err(format!(
            "{description} failed. Check installation, network access and the requested release."
        ));
    }
    String::from_utf8(result.stdout).map_err(|_| format!("{description} returned invalid UTF-8"))
}

fn read(path: &Path) -> Result<String> {
    let metadata = fs::symlink_metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 8_000_000 {
        return Err(format!(
            "Expected a regular file under 8 MB: {}",
            path.display()
        ));
    }
    fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
}

fn write(path: &Path, value: impl AsRef<[u8]>) -> Result<()> {
    if let Ok(meta) = fs::symlink_metadata(path)
        && (!meta.is_file() || meta.file_type().is_symlink())
    {
        return Err(format!("Refusing to overwrite {}", path.display()));
    }
    fs::write(path, value).map_err(|e| format!("{}: {e}", path.display()))
}

/// Only remove directories exclusively created by this invocation.
struct Temporary(PathBuf);
impl Temporary {
    fn in_directory(parent: &Path) -> Result<Self> {
        for index in 0..1000 {
            let name = format!(
                ".bracel-{}-{}-{index}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(|e| e.to_string())?
                    .as_nanos()
            );
            let path = parent.join(name);
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self(path)),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(format!("Cannot create temporary directory: {e}")),
            }
        }
        Err("Cannot allocate temporary directory".into())
    }
}
impl Drop for Temporary {
    fn drop(&mut self) {
        // Git object files are read-only on Windows. Unset that attribute only
        // on files in our owned temporary tree, without following symlinks.
        #[cfg(windows)]
        #[allow(clippy::permissions_set_readonly_false)] // Windows file attribute; never compiled on Unix.
        fn writable(path: &Path) {
            if let Ok(meta) = fs::symlink_metadata(path) {
                if meta.file_type().is_symlink() {
                    return;
                }
                if meta.is_dir() {
                    if let Ok(entries) = fs::read_dir(path) {
                        for entry in entries.flatten() {
                            writable(&entry.path());
                        }
                    }
                } else {
                    let mut permissions = meta.permissions();
                    permissions.set_readonly(false);
                    let _ = fs::set_permissions(path, permissions);
                }
            }
        }
        #[cfg(windows)]
        writable(&self.0);
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn mode_args(args: &[String]) -> Result<(Option<Mode>, Vec<String>)> {
    let mut mode = None;
    let mut rest = Vec::new();
    for arg in args {
        let selected = match arg.as_str() {
            "--native" => Some(Mode::Native),
            "--docker" => Some(Mode::Docker),
            _ => None,
        };
        if let Some(selected) = selected {
            if mode.replace(selected).is_some() {
                return Err("Choose --native or --docker once".into());
            }
        } else {
            rest.push(arg.clone());
        }
    }
    Ok((mode, rest))
}

fn setup(root: &Path, mode: Mode) -> Result<()> {
    let mut missing = false;
    let checks: &[(&str, &[&str], &str)] = &[
        (
            "git",
            &["--version"],
            "Install Git: https://git-scm.com/downloads",
        ),
        (
            "docker",
            &["compose", "version"],
            "Install Docker Desktop (or Docker Engine with Compose): https://docs.docker.com/get-started/get-docker/",
        ),
        (
            "docker",
            &["info"],
            "Start Docker Desktop / the Docker daemon, then run bracel setup again.",
        ),
    ];
    for (program, args, help) in checks {
        let ok = Command::new(program)
            .args(*args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success());
        println!(
            "{} {program} {}",
            if ok { "OK" } else { "MISSING" },
            args.join(" ")
        );
        if !ok {
            println!("  {help}");
            missing = true;
        }
    }
    if mode == Mode::Native {
        let temp = Temporary::in_directory(&std::env::temp_dir())?;
        write(&temp.0.join("probe.rs"), "fn main() {}\n")?;
        let ok = project::native_command(
            root,
            "rustc",
            &[
                temp.0.join("probe.rs").to_string_lossy().into_owned(),
                "-o".into(),
                temp.0.join("probe.exe").to_string_lossy().into_owned(),
            ],
        )
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success());
        println!(
            "{} Rust compiler and linker",
            if ok { "OK" } else { "MISSING" }
        );
        if !ok {
            println!(
                "  Install Rust: https://rustup.rs\n  Windows: Visual Studio Build Tools, Desktop development with C++ (Windows SDK and CMake).\n  macOS: xcode-select --install. Ubuntu/Debian: build-essential pkg-config libssl-dev cmake."
            );
            missing = true;
        }
    }
    if missing {
        return Err(
            "Finish the steps above, open a new terminal, and run bracel setup again.".into(),
        );
    }
    if root.join("Cargo.toml").exists() {
        let mut config = project::Settings::load(root)?;
        config.mode = mode;
        config.save(root)?;
    }
    println!(
        "Ready. {}",
        if root.join("Cargo.toml").exists() {
            "Run bracel dev."
        } else {
            "Run bracel new my-api, then cd my-api and bracel dev."
        }
    );
    Ok(())
}

pub fn run(args: &[String]) -> ExitCode {
    let result = (|| {
        let root = std::env::current_dir().map_err(|e| e.to_string())?;
        let action = args.first().ok_or(crate::USAGE)?;
        // Options after an application's command belong to the application.
        if matches!(action.as_str(), "run" | "cargo") {
            let mut index = 1;
            let mode = match args.get(index).map(String::as_str) {
                Some("--native") => {
                    index += 1;
                    Some(Mode::Native)
                }
                Some("--docker") => {
                    index += 1;
                    Some(Mode::Docker)
                }
                _ => None,
            };
            if index == args.len() {
                return Err(crate::USAGE.into());
            }
            return project::invoke(&root, mode, action == "cargo", &args[index..]);
        }
        let (mode, rest) = mode_args(&args[1..])?;
        match action.as_str() {
            "setup" if rest.is_empty() => {
                setup(&root, mode.unwrap_or(project::Settings::load(&root)?.mode))
            }
            "new" if rest.len() == 1 => project::create(&rest[0], mode.unwrap_or(Mode::Docker)),
            "dev" => project::dev(&root, mode, &rest),
            "down" if rest.is_empty() && mode.is_none() => project::down(&root),
            "upgrade" => upgrade::run(&root, mode, &rest),
            "self" if mode.is_none() => release::self_update(&rest),
            _ => Err(crate::USAGE.into()),
        }
    })();
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

/// Keep the existing AI interface usable on machines with no host Rust toolchain.
pub fn ai(args: &[String]) -> Option<ExitCode> {
    if args.is_empty()
        || matches!(args[0].as_str(), "help" | "--help")
        || std::env::var_os("BRACEL_DOCKER_CHILD").is_some()
    {
        return None;
    }
    let mut root = std::env::current_dir().ok()?;
    let mut forwarded = Vec::new();
    let mut index = 0;
    while index < args.len() {
        if args[index] == "--root" {
            index += 1;
            root = PathBuf::from(args.get(index)?);
        } else {
            forwarded.push(args[index].clone());
        }
        index += 1;
    }
    if !root.join(".bracel/project.json").is_file() {
        return None;
    }
    let result: Result<bool> = (|| {
        if project::Settings::load(&root)?.mode != Mode::Docker {
            return Ok(false);
        }
        project::docker_ai(&root, &forwarded)?;
        Ok(true)
    })();
    match result {
        Ok(false) => None,
        Ok(true) => Some(ExitCode::SUCCESS),
        Err(error) => {
            eprintln!("{error}");
            Some(ExitCode::FAILURE)
        }
    }
}
