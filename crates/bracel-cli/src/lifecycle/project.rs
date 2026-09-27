use super::*;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, io::Write, net::TcpListener};
use toml_edit::{DocumentMut, value};

#[derive(Serialize, Deserialize)]
pub(super) struct Settings {
    schema_version: u32,
    pub mode: Mode,
    api_port: u16,
    database_port: u16,
    mail_port: u16,
    mail_ui_port: u16,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: 1,
            mode: Mode::Docker,
            api_port: 3000,
            database_port: 5432,
            mail_port: 1025,
            mail_ui_port: 8025,
        }
    }
}
impl Settings {
    pub fn load(root: &Path) -> Result<Self> {
        let path = root.join(".bracel/project.json");
        if !path.exists() {
            return Ok(Self::default());
        }
        let settings: Self = serde_json::from_str(&read(&path)?)
            .map_err(|e| format!("Invalid .bracel/project.json: {e}"))?;
        if settings.schema_version != 1
            || [
                settings.api_port,
                settings.database_port,
                settings.mail_port,
                settings.mail_ui_port,
            ]
            .contains(&0)
        {
            return Err("Unsupported project settings or zero port in .bracel/project.json".into());
        }
        Ok(settings)
    }
    pub fn save(&self, root: &Path) -> Result<()> {
        directory(root, ".bracel")?;
        write(
            &root.join(".bracel/project.json"),
            serde_json::to_string_pretty(self).map_err(|e| e.to_string())? + "\n",
        )
    }
}

fn directory(root: &Path, relative: &str) -> Result<PathBuf> {
    let mut path = root.to_path_buf();
    for part in Path::new(relative).components() {
        path.push(part);
        match fs::symlink_metadata(&path) {
            Ok(meta) if !meta.is_dir() || meta.file_type().is_symlink() => {
                return Err(format!("Expected a project directory: {}", path.display()));
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&path).map_err(|e| e.to_string())?
            }
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(path)
}

fn available_port(preferred: u16) -> Result<u16> {
    TcpListener::bind(("127.0.0.1", preferred))
        .or_else(|_| TcpListener::bind(("127.0.0.1", 0)))
        .and_then(|s| s.local_addr())
        .map(|a| a.port())
        .map_err(|e| format!("Cannot select a development port: {e}"))
}

fn valid_name(name: &str) -> bool {
    name.len() <= 64
        && name.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
        && name
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-' || c == b'_')
        && !matches!(
            name,
            "self"
                | "super"
                | "crate"
                | "mod"
                | "fn"
                | "type"
                | "async"
                | "await"
                | "move"
                | "ref"
                | "pub"
                | "use"
                | "match"
                | "loop"
                | "true"
                | "false"
                | "const"
                | "static"
                | "enum"
                | "struct"
                | "trait"
                | "impl"
                | "where"
                | "let"
                | "if"
                | "else"
                | "in"
                | "for"
                | "while"
                | "break"
                | "continue"
                | "return"
                | "as"
                | "extern"
                | "dyn"
                | "unsafe"
                | "box"
                | "do"
                | "try"
                | "yield"
                | "gen"
                | "abstract"
                | "become"
                | "final"
                | "macro"
                | "override"
                | "priv"
                | "typeof"
                | "unsized"
                | "virtual"
                | "con"
                | "prn"
                | "aux"
                | "nul"
                | "test"
                | "build"
                | "deps"
                | "examples"
                | "incremental"
        )
        && !(name.len() == 4
            && (name.starts_with("com") || name.starts_with("lpt"))
            && name.as_bytes()[3].is_ascii_digit())
}

pub fn create(destination: &str, mode: Mode) -> Result<()> {
    let destination = std::path::absolute(destination).map_err(|e| e.to_string())?;
    let name = destination.file_name().and_then(|n| n.to_str()).filter(|n| valid_name(n))
        .ok_or("Use a project name starting with a lowercase letter and containing letters, numbers, hyphens or underscores; Rust keywords and reserved names are unsupported.")?;
    if destination.exists() || fs::symlink_metadata(&destination).is_ok() {
        return Err("Destination already exists; choose a new directory.".into());
    }
    let parent = destination.parent().ok_or("Missing destination parent")?;
    if !parent.is_dir() {
        return Err("Create the parent directory first.".into());
    }
    let temporary = Temporary::in_directory(parent)?;
    let stage = temporary.0.join("application");
    println!("Creating {name} from Bracel {}…", env!("CARGO_PKG_VERSION"));
    status(
        Command::new("git")
            .args([
                "-c",
                "advice.detachedHead=false",
                "clone",
                "--depth",
                "1",
                "--origin",
                "starter",
                "--branch",
                concat!("v", env!("CARGO_PKG_VERSION")),
                "--",
                STARTER,
            ])
            .arg(&stage),
        "Download matching starter (install Git and check GitHub access)",
    )?;
    status(
        Command::new("git")
            .arg("-C")
            .arg(&stage)
            .args(["switch", "-c", "main"]),
        "Create application branch",
    )?;
    rename(&stage, name)?;
    let settings = Settings {
        mode,
        api_port: available_port(3000)?,
        database_port: available_port(5432)?,
        mail_port: available_port(1025)?,
        mail_ui_port: available_port(8025)?,
        ..Settings::default()
    };
    settings.save(&stage)?;
    environment_file(&stage, &settings)?;
    fs::rename(&stage, &destination).map_err(|e| format!("Cannot finish project creation: {e}"))?;
    println!(
        "Created {}\n\n  cd \"{}\"\n  bracel dev\n\nAPI: http://127.0.0.1:{}\nMail: http://127.0.0.1:{}\nThe starter remote is retained. Add your own origin when ready to publish your application.",
        destination.display(),
        destination.display(),
        settings.api_port,
        settings.mail_ui_port
    );
    Ok(())
}

fn rename(root: &Path, name: &str) -> Result<()> {
    let mut manifest = read(&root.join("Cargo.toml"))?
        .parse::<DocumentMut>()
        .map_err(|e| e.to_string())?;
    if manifest["package"]["name"].as_str() != Some("bracel-starter") {
        return Err("The selected starter has an unsupported package layout".into());
    }
    manifest["package"]["name"] = value(name);
    manifest["package"]["default-run"] = value(name);
    write(&root.join("Cargo.toml"), manifest.to_string())?;
    let mut lock = read(&root.join("Cargo.lock"))?
        .parse::<DocumentMut>()
        .map_err(|e| e.to_string())?;
    for package in lock["package"]
        .as_array_of_tables_mut()
        .ok_or("Invalid starter lockfile")?
        .iter_mut()
    {
        if package.get("name").and_then(|v| v.as_str()) == Some("bracel-starter")
            && !package.contains_key("source")
        {
            package["name"] = value(name);
        }
    }
    write(&root.join("Cargo.lock"), lock.to_string())?;
    fn replace(root: &Path, name: &str) -> Result<()> {
        if !root.exists() {
            return Ok(());
        }
        let metadata = fs::symlink_metadata(root).map_err(|e| e.to_string())?;
        if metadata.file_type().is_symlink() {
            return Err("Starter source contains an unsupported symlink".into());
        }
        if metadata.is_dir() {
            for entry in fs::read_dir(root).map_err(|e| e.to_string())? {
                replace(&entry.map_err(|e| e.to_string())?.path(), name)?;
            }
        } else {
            let bytes = fs::read(root).map_err(|e| e.to_string())?;
            if let Ok(text) = String::from_utf8(bytes) {
                let updated = text
                    .replace("bracel_starter", &name.replace('-', "_"))
                    .replace("bracel-starter", name);
                if text != updated {
                    write(root, updated)?;
                }
            }
        }
        Ok(())
    }
    for folder in ["src", "tests", "scripts", ".github", "Dockerfile"] {
        replace(&root.join(folder), name)?;
    }
    Ok(())
}

fn environment_file(root: &Path, settings: &Settings) -> Result<()> {
    if root.join(".env").exists() {
        return Ok(());
    }
    let content = read(&root.join(".env.example"))?
        .replace(
            "127.0.0.1:5432",
            &format!("127.0.0.1:{}", settings.database_port),
        )
        .replace(
            "127.0.0.1:3000",
            &format!("127.0.0.1:{}", settings.api_port),
        )
        .replace(
            "MAIL_LOCAL_PORT=1025",
            &format!("MAIL_LOCAL_PORT={}", settings.mail_port),
        );
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(root.join(".env"))
        .and_then(|mut f| f.write_all(content.as_bytes()))
        .map_err(|e| format!("Cannot create .env: {e}"))
}

/// Parse dotenv values without invoking a shell; process variables take priority.
fn environment(root: &Path) -> Result<BTreeMap<String, String>> {
    let mut values = BTreeMap::new();
    let path = root.join(".env");
    if path.exists() {
        let content = read(&path)?;
        let mut lines = content.lines().enumerate();
        while let Some((number, line)) = lines.next() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let line = line.strip_prefix("export ").unwrap_or(line);
            let invalid = || {
                format!(
                    "Invalid .env assignment at line {}. Use KEY=value, optionally quoted; shell execution and interpolation are unsupported.",
                    number + 1
                )
            };
            let (key, raw) = line.split_once('=').ok_or_else(invalid)?;
            let key = key.trim();
            if key.is_empty()
                || !key.bytes().enumerate().all(|(i, c)| {
                    c == b'_' || c.is_ascii_alphabetic() || (i > 0 && c.is_ascii_digit())
                })
            {
                return Err(invalid());
            }
            let raw = raw.trim();
            let value = if raw.starts_with(['\'', '"']) {
                let quote = raw.chars().next().ok_or_else(invalid)?;
                let mut value = raw[1..].to_string();
                loop {
                    if let Some(end) = value.find(quote) {
                        let trailing = value[end + 1..].trim();
                        if !trailing.is_empty() && !trailing.starts_with('#') {
                            return Err(invalid());
                        }
                        value.truncate(end);
                        break value;
                    }
                    let (_, next) = lines.next().ok_or_else(invalid)?;
                    value.push('\n');
                    value.push_str(next);
                }
            } else {
                raw.split_once(" #")
                    .map(|(v, _)| v)
                    .unwrap_or(raw)
                    .trim_end()
                    .to_string()
            };
            values.insert(key.to_string(), std::env::var(key).unwrap_or(value));
        }
    }
    for key in [
        "DATABASE_URL",
        "BIND_ADDR",
        "MAIL_LOCAL_PORT",
        "MAIL_SMTP_HOST",
        "MAIL_SMTP_USERNAME",
        "MAIL_SMTP_PASSWORD",
        "TEST_DATABASE_URL",
    ] {
        if let Ok(value) = std::env::var(key) {
            values.insert(key.into(), value);
        }
    }
    Ok(values)
}

pub fn native_command(root: &Path, program: &str, args: &[String]) -> Command {
    #[cfg(windows)]
    if root.join("scripts/windows.ps1").is_file() {
        let mut command = Command::new("powershell.exe");
        command
            .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
            .arg(root.join("scripts/windows.ps1"))
            .arg(program)
            .args(args)
            .current_dir(root);
        return command;
    }
    let mut command = Command::new(program);
    command.args(args).current_dir(root);
    command
}

fn package(root: &Path) -> Result<String> {
    let doc = read(&root.join("Cargo.toml"))?
        .parse::<DocumentMut>()
        .map_err(|e| e.to_string())?;
    doc.get("package")
        .and_then(|p| p.get("default-run").or_else(|| p.get("name")))
        .and_then(|v| v.as_str())
        .filter(|n| valid_name(n))
        .map(String::from)
        .ok_or(
            "Run this command in a Bracel application root with a valid package/default-run name."
                .into(),
        )
}

fn compose(root: &Path, settings: &Settings) -> Result<Command> {
    let name = package(root)?;
    let toolchain: toml::Value =
        toml::from_str(&read(&root.join("rust-toolchain.toml"))?).map_err(|e| e.to_string())?;
    let channel = toolchain
        .get("toolchain")
        .and_then(|t| t.get("channel"))
        .and_then(|v| v.as_str())
        .ok_or("Missing pinned Rust toolchain")?;
    if !channel.bytes().all(|b| b.is_ascii_digit() || b == b'.') {
        return Err(
            "Docker development requires a numeric Rust version in rust-toolchain.toml.".into(),
        );
    }
    let root = fs::canonicalize(root).map_err(|e| e.to_string())?;
    let source = root
        .to_string_lossy()
        .trim_start_matches("\\\\?\\")
        .replace('\\', "/");
    let hash = format!("{:x}", Sha256::digest(source.as_bytes()));
    let project = format!("{}-{}", name.replace('_', "-"), &hash[..8]);
    let mut env = environment(&root)?;
    let mut tools_env = BTreeMap::from([
        ("CARGO_TARGET_DIR".to_string(), "/build".to_string()),
        ("BRACEL_DOCKER_CHILD".to_string(), "1".to_string()),
    ]);
    if let Some(value) = env.get("TEST_DATABASE_URL") {
        tools_env.insert("TEST_DATABASE_URL".into(), value.clone());
    }
    if let Ok(value) = std::env::var("CARGO_BUILD_JOBS") {
        tools_env.insert("CARGO_BUILD_JOBS".into(), value);
    }
    // Container development always uses its isolated database and mail capture.
    env.insert(
        "DATABASE_URL".into(),
        "postgres://starter:starter@postgres:5432/starter".into(),
    );
    env.insert("BIND_ADDR".into(), "0.0.0.0:3000".into());
    env.insert("MAIL_LOCAL_PORT".into(), "1025".into());
    env.remove("MAIL_SMTP_HOST");
    env.remove("MAIL_SMTP_USERNAME");
    env.remove("MAIL_SMTP_PASSWORD");
    let volumes = json!([
        {"type":"bind","source":source,"target":"/app"},
        "cargo-cache:/usr/local/cargo", "rustup-cache:/usr/local/rustup", "build-cache:/build", "cli-cache:/bracel-tools"
    ]);
    // Linux bind mounts retain numeric ownership. Initialize only the cache
    // volumes, then run as the application owner so generated files stay editable.
    let entrypoint = "set -eu; uid=$(stat -c %u /app/Cargo.toml); gid=$(stat -c %g /app/Cargo.toml); for dir in /usr/local/cargo /usr/local/rustup /build /bracel-tools; do if [ \"$(stat -c %u:%g \"$dir\")\" != \"$uid:$gid\" ]; then chown -R \"$uid:$gid\" \"$dir\"; fi; done; export HOME=/bracel-tools; if [ \"$uid\" = 0 ]; then exec \"$@\"; else exec setpriv --reuid=\"$uid\" --regid=\"$gid\" --clear-groups \"$@\"; fi";
    let tools = json!({"image":format!("rust:{channel}-bookworm"), "working_dir":"/app", "volumes":volumes,
        "entrypoint":["sh","-c",entrypoint,"bracel-dev"],
        "environment":tools_env, "profiles":["tools"]});
    let mut app = tools.clone();
    app.as_object_mut()
        .ok_or("Invalid service")?
        .remove("profiles");
    // Mailer::local deliberately accepts loopback only. Sharing Mailpit's
    // network namespace preserves that contract without weakening SMTP policy.
    app["network_mode"] = json!("service:mailpit");
    env.insert("CARGO_TARGET_DIR".into(), "/build".into());
    env.insert("BRACEL_DOCKER_CHILD".into(), "1".into());
    app["environment"] = json!(env);
    app["init"] = json!(true);
    app["command"] = json!([format!("/build/debug/{name}"), "serve"]);
    let mut worker = app.clone();
    worker["command"] = json!([format!("/build/debug/{name}"), "auth:mail-work"]);
    let config = json!({"name":project,"services":{
        "postgres":{"image":"postgres:18.6-bookworm", "environment":{"POSTGRES_USER":"starter", "POSTGRES_PASSWORD":"starter", "POSTGRES_DB":"starter"},
            "ports":[format!("127.0.0.1:{}:5432", settings.database_port)], "volumes":["postgres-data:/var/lib/postgresql"],
            "healthcheck":{"test":["CMD-SHELL","pg_isready -U starter -d starter"],"interval":"2s","timeout":"3s","retries":30}},
        "mailpit":{"image":"axllent/mailpit:v1.31.2", "ports":[format!("127.0.0.1:{}:1025", settings.mail_port),format!("127.0.0.1:{}:8025",settings.mail_ui_port),format!("127.0.0.1:{}:3000",settings.api_port)]},
        "tools":tools,"app":app,"worker":worker},
        "volumes":{"postgres-data":{},"cargo-cache":{},"rustup-cache":{},"build-cache":{},"cli-cache":{}}});
    let folder = directory(&root, "target/bracel")?;
    let file = folder.join("compose.json");
    // Compose interpolation must not reinterpret literal values from .env.
    write(
        &file,
        serde_json::to_string_pretty(&config)
            .map_err(|e| e.to_string())?
            .replace('$', "$$"),
    )?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).map_err(|e| e.to_string())?;
    }
    let mut command = Command::new("docker");
    command
        .current_dir(&root)
        .args(["compose", "--project-name", &project, "--project-directory"])
        .arg(&root)
        .arg("--env-file")
        .arg(empty_env(&folder)?)
        .arg("-f")
        .arg(file);
    Ok(command)
}

fn empty_env(folder: &Path) -> Result<PathBuf> {
    let file = folder.join("empty.env");
    write(&file, "")?;
    Ok(file)
}

pub fn cargo(root: &Path, mode: Mode, args: &[String]) -> Result<()> {
    if mode == Mode::Native {
        return status(
            &mut native_command(root, "cargo", args),
            "Cargo (run bracel setup --native to check your tools)",
        );
    }
    status(
        compose(root, &Settings::load(root)?)?
            .args(["run", "--rm", "--no-deps", "tools", "cargo"])
            .args(args),
        "Cargo in Docker",
    )
}

fn application(root: &Path, mode: Mode, args: &[String], build: bool) -> Result<()> {
    let name = package(root)?;
    if mode == Mode::Native {
        let mut command = native_command(
            root,
            "cargo",
            &[
                vec![
                    "run".into(),
                    "--locked".into(),
                    "--bin".into(),
                    name,
                    "--".into(),
                ],
                args.to_vec(),
            ]
            .concat(),
        );
        command.envs(environment(root)?);
        return status(&mut command, "Application command");
    }
    if build {
        cargo(
            root,
            mode,
            &[
                "build".into(),
                "--locked".into(),
                "--bin".into(),
                name.clone(),
            ],
        )?;
    }
    let settings = Settings::load(root)?;
    status(
        compose(root, &settings)?.args(["up", "-d", "--wait", "postgres", "mailpit"]),
        "Start development services",
    )?;
    status(
        compose(root, &settings)?
            .args([
                "run",
                "--rm",
                "--no-deps",
                "app",
                &format!("/build/debug/{name}"),
            ])
            .args(args),
        "Application in Docker",
    )
}

pub fn invoke(root: &Path, mode: Option<Mode>, is_cargo: bool, args: &[String]) -> Result<()> {
    let mode = mode.unwrap_or(Settings::load(root)?.mode);
    package(root)?;
    if is_cargo {
        cargo(root, mode, args)
    } else {
        application(root, mode, args, true)
    }
}

pub fn dev(root: &Path, mode: Option<Mode>, args: &[String]) -> Result<()> {
    if args
        .iter()
        .any(|a| a != "--no-services" && a != "--no-migrate")
    {
        return Err(crate::USAGE.into());
    }
    let settings = Settings::load(root)?;
    let mode = mode.unwrap_or(settings.mode);
    let name = package(root)?;
    let no_services = args.iter().any(|a| a == "--no-services");
    let no_migrate = args.iter().any(|a| a == "--no-migrate");
    if mode == Mode::Docker && no_services {
        return Err(
            "--no-services applies to native development with an existing local database.".into(),
        );
    }
    environment_file(root, &settings)?;
    if mode == Mode::Native && !no_migrate {
        let env = environment(root)?;
        let database = env.get("DATABASE_URL").ok_or("Set DATABASE_URL in .env")?;
        let authority = database
            .strip_prefix("postgres://")
            .or_else(|| database.strip_prefix("postgresql://"))
            .and_then(|v| v.split('/').next())
            .unwrap_or("")
            .rsplit('@')
            .next()
            .unwrap_or("");
        let host = authority.split('?').next().unwrap_or("");
        if database.contains('?')
            || !(host == "localhost"
                || host.starts_with("localhost:")
                || host == "127.0.0.1"
                || host.starts_with("127.0.0.1:")
                || host == "[::1]"
                || host.starts_with("[::1]:"))
        {
            return Err("Automatic dev migrations require a loopback database URL without query overrides. Use --no-migrate, and run bracel run --native migrate explicitly for another database.".into());
        }
    }
    if !no_services {
        let mut command = compose(root, &settings)?;
        // Native serving owns its API port, so do not publish that port through Mailpit.
        if mode == Mode::Native {
            native_compose_ports(root)?;
        }
        status(
            command.args(["up", "-d", "--wait", "postgres", "mailpit"]),
            "Start PostgreSQL and local mail (run bracel setup if Docker is unavailable)",
        )?;
    }
    println!("Building {name}. The first build downloads and compiles dependencies…");
    cargo(
        root,
        mode,
        &["build".into(), "--locked".into(), "--bin".into(), name],
    )?;
    if !no_migrate {
        application(root, mode, &["migrate".into()], false)?;
    }
    if mode == Mode::Docker {
        status(
            compose(root, &settings)?.args(["up", "-d", "--no-deps", "worker"]),
            "Start account mail worker",
        )?;
    } else {
        println!(
            "For account email delivery, run bracel run --native auth:mail-work in another terminal."
        );
    }
    println!(
        "API: http://127.0.0.1:{}\nMail: http://127.0.0.1:{}\nPress Ctrl+C to stop the API. bracel down stops services and retains your database.",
        settings.api_port, settings.mail_ui_port
    );
    application(root, mode, &["serve".into()], false)
}

fn native_compose_ports(root: &Path) -> Result<()> {
    let file = root.join("target/bracel/compose.json");
    let mut config: serde_json::Value =
        serde_json::from_str(&read(&file)?).map_err(|e| e.to_string())?;
    config["services"]["mailpit"]["ports"]
        .as_array_mut()
        .ok_or("Invalid ports")?
        .retain(|p| !p.as_str().unwrap_or("").ends_with(":3000"));
    write(
        &file,
        serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?,
    )
}

pub fn down(root: &Path) -> Result<()> {
    let file = root.join("target/bracel/compose.json");
    if !file.exists() {
        println!("No Bracel development services have been started in this directory.");
        return Ok(());
    }
    read(&file)?;
    status(
        Command::new("docker")
            .current_dir(root)
            .args(["compose", "--project-directory"])
            .arg(root)
            .arg("--env-file")
            .arg(root.join("target/bracel/empty.env"))
            .arg("-f")
            .arg(file)
            .args(["down", "--remove-orphans"]),
        "Stop development services (database retained)",
    )
}

pub fn docker_ai(root: &Path, args: &[String]) -> Result<()> {
    let settings = Settings::load(root)?;
    let version = env!("CARGO_PKG_VERSION");
    let install = format!(
        "set -e; if ! cargo metadata --locked --offline --format-version 1 >/dev/null 2>&1; then cargo fetch --locked; fi; if [ \"$(/bracel-tools/bin/bracel --version 2>/dev/null || true)\" != 'bracel {version}' ]; then cargo install --git {FRAMEWORK} --tag v{version} bracel-cli --locked --root /bracel-tools --force; fi"
    );
    status(
        compose(root, &settings)?
            .args([
                "run",
                "--rm",
                "--no-deps",
                "-T",
                "tools",
                "sh",
                "-c",
                &install,
            ])
            .stdout(std::io::stderr()),
        "Prepare AI companion in Docker",
    )?;
    status(
        compose(root, &settings)?
            .args([
                "run",
                "--rm",
                "--no-deps",
                "-T",
                "tools",
                "/bracel-tools/bin/bracel",
                "ai",
            ])
            .args(args),
        "AI companion in Docker",
    )
}
