use super::*;
use sha2::{Digest, Sha256};

pub fn version(input: &str) -> Result<(u64, u64, u64)> {
    let parts: Vec<_> = input.trim_start_matches('v').split('.').collect();
    if parts.len() != 3
        || parts.iter().any(|p| {
            p.is_empty()
                || (p.len() > 1 && p.starts_with('0'))
                || !p.bytes().all(|b| b.is_ascii_digit())
        })
    {
        return Err("Use a stable release version, for example 0.4.0.".into());
    }
    let parse = |p: &str| {
        p.parse::<u64>()
            .map_err(|_| "Invalid release version".to_string())
    };
    Ok((parse(parts[0])?, parse(parts[1])?, parse(parts[2])?))
}

pub fn select(requested: Option<&str>) -> Result<String> {
    if let Some(requested) = requested {
        version(requested)?;
        return Ok(requested.trim_start_matches('v').to_string());
    }
    let tags = output(
        Command::new("git").args(["ls-remote", "--tags", "--refs", FRAMEWORK]),
        "Find Bracel releases",
    )?;
    tags.lines()
        .filter_map(|line| line.split_once("refs/tags/v").map(|(_, v)| v))
        .filter_map(|v| version(v).ok().map(|parsed| (parsed, v.to_string())))
        .max_by_key(|(v, _)| *v)
        .map(|(_, v)| v)
        .ok_or("No stable Bracel releases found".into())
}

pub fn checkout(version: &str) -> Result<(Temporary, PathBuf, String)> {
    let temporary = Temporary::in_directory(&std::env::temp_dir())?;
    let source = temporary.0.join("release");
    status(
        Command::new("git")
            .args([
                "-c",
                "advice.detachedHead=false",
                "clone",
                "--quiet",
                "--depth",
                "1",
                "--branch",
                &format!("v{version}"),
                "--",
                FRAMEWORK,
            ])
            .arg(&source),
        "Download selected framework release",
    )?;
    let revision = output(
        Command::new("git")
            .arg("-C")
            .arg(&source)
            .args(["rev-parse", "HEAD"]),
        "Resolve release commit",
    )?
    .trim()
    .to_string();
    if revision.len() != 40 || !revision.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("Invalid release commit".into());
    }
    Ok((temporary, source, revision))
}

fn download(url: &str, path: &Path) -> Result<()> {
    status(
        Command::new(if cfg!(windows) { "curl.exe" } else { "curl" })
            .args([
                "--fail",
                "--silent",
                "--show-error",
                "--location",
                "--proto",
                "=https",
                "--proto-redir",
                "=https",
                "--connect-timeout",
                "20",
                "--max-time",
                "180",
                "--retry",
                "2",
                "--output",
            ])
            .arg(path)
            .arg(url),
        "Download release asset (install curl or use the Bracel installer)",
    )
}

pub fn self_update(args: &[String]) -> Result<()> {
    let requested = match args {
        [action] if action == "update" => None,
        [action, option, value] if action == "update" && option == "--to" => Some(value.as_str()),
        _ => return Err(crate::USAGE.into()),
    };
    let selected = if let Some(requested) = requested {
        select(Some(requested))?
    } else {
        // A framework tag may precede publication of its CLI assets. Follow the
        // published release channel used by both installers for self updates.
        let metadata = Temporary::in_directory(&std::env::temp_dir())?;
        let path = metadata.0.join("release.json");
        download(
            "https://api.github.com/repos/4H1R/bracel/releases/latest",
            &path,
        )?;
        let release: serde_json::Value =
            serde_json::from_str(&read(&path)?).map_err(|e| e.to_string())?;
        select(Some(
            release
                .get("tag_name")
                .and_then(|v| v.as_str())
                .ok_or("The latest release has no version tag")?,
        ))?
    };
    if selected == env!("CARGO_PKG_VERSION") {
        println!(
            "Bracel CLI {selected} is already installed. Application dependencies are unchanged."
        );
        return Ok(());
    }
    let target = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => "x86_64-pc-windows-msvc.exe",
        ("linux", "x86_64") => "x86_64-unknown-linux-gnu",
        ("macos", "aarch64") => "aarch64-apple-darwin",
        ("macos", "x86_64") => "x86_64-apple-darwin",
        _ => return Err("No prebuilt CLI for this platform. Install the selected release with cargo install --git https://github.com/4H1R/bracel --tag vVERSION bracel-cli --locked --force.".into()),
    };
    let current = std::env::current_exe().map_err(|e| e.to_string())?;
    let parent = current
        .parent()
        .ok_or("Cannot find CLI installation directory")?;
    let temporary = Temporary::in_directory(parent).map_err(|_| "The CLI installation directory is not writable. Re-run the installer into a user-owned directory.".to_string())?;
    let asset = format!("bracel-{target}");
    let base = format!("{FRAMEWORK}/releases/download/v{selected}");
    let checksums = temporary.0.join("SHA256SUMS");
    let binary = temporary.0.join(&asset);
    download(&format!("{base}/SHA256SUMS"), &checksums)?;
    download(&format!("{base}/{asset}"), &binary)?;
    let sums = read(&checksums)?;
    let expected: Vec<_> = sums
        .lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let hash = parts.next()?;
            (parts.next()?.trim_start_matches('*') == asset).then_some(hash)
        })
        .collect();
    let actual = format!(
        "{:x}",
        Sha256::digest(fs::read(&binary).map_err(|e| e.to_string())?)
    );
    if expected != [actual.as_str()] {
        return Err("Release checksum mismatch; the installed CLI was preserved.".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
    }
    if output(
        Command::new(&binary).arg("--version"),
        "Verify downloaded CLI",
    )?
    .trim()
        != format!("bracel {selected}")
    {
        return Err("Downloaded CLI has an unexpected version; installed CLI preserved.".into());
    }
    let previous = current.with_extension("previous");
    if previous.exists() {
        fs::remove_file(&previous).map_err(|_| {
            "Cannot remove the previous CLI backup; close other Bracel processes and retry"
                .to_string()
        })?;
    }
    fs::rename(&current, &previous).map_err(|_| {
        "Cannot replace the running CLI. Close other Bracel processes and re-run the installer."
            .to_string()
    })?;
    if let Err(error) = fs::rename(&binary, &current) {
        fs::rename(&previous, &current).map_err(|restore| {
            format!(
                "Update failed ({error}); restore {} to {} manually: {restore}",
                previous.display(),
                current.display()
            )
        })?;
        return Err(format!("Update failed; previous CLI restored: {error}"));
    }
    println!(
        "Installed Bracel CLI {selected}. Previous binary: {}\nApplication dependencies are unchanged. Run bracel upgrade --check inside an application.",
        previous.display()
    );
    Ok(())
}
