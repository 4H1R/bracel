use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

pub type Result<T> = std::result::Result<T, String>;

pub fn hash(bytes: impl AsRef<[u8]>) -> String {
    format!("{:x}", Sha256::digest(bytes.as_ref()))
}

pub fn text(path: &Path) -> Result<String> {
    let metadata =
        fs::symlink_metadata(path).map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > 2_000_000 {
        return Err(format!(
            "Expected a regular file under 2 MB: {}",
            path.display()
        ));
    }
    fs::read_to_string(path)
        .map(|content| content.replace("\r\n", "\n"))
        .map_err(|e| format!("Cannot read {}: {e}", path.display()))
}

pub fn relative(path: &Path) -> Result<()> {
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(format!(
            "Expected a contained relative path: {}",
            path.display()
        ));
    }
    Ok(())
}

pub fn contained(root: &Path, relative_path: &str) -> Result<PathBuf> {
    let path = Path::new(relative_path);
    relative(path)?;
    let mut current = root.to_path_buf();
    for part in path.components() {
        current.push(part);
        if let Ok(meta) = fs::symlink_metadata(&current)
            && meta.file_type().is_symlink()
        {
            return Err(format!(
                "Symlink is not an owned project path: {}",
                current.display()
            ));
        }
    }
    Ok(current)
}

pub fn slash(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

pub fn files(root: &Path, folder: &str) -> Result<Vec<PathBuf>> {
    fn visit(root: &Path, out: &mut Vec<PathBuf>, depth: usize) -> Result<()> {
        if !root.exists() {
            return Ok(());
        }
        if depth > 24 || out.len() > 20000 {
            return Err("Source traversal limit exceeded".into());
        }
        let meta = fs::symlink_metadata(root).map_err(|e| e.to_string())?;
        if meta.file_type().is_symlink() {
            return Ok(());
        }
        if meta.is_file() {
            out.push(root.to_path_buf());
            return Ok(());
        }
        for entry in fs::read_dir(root).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if matches!(
                name.as_ref(),
                "target"
                    | "node_modules"
                    | ".git"
                    | ".scratch"
                    | ".bracel"
                    | ".agents"
                    | ".codex"
                    | ".claude"
                    | ".cursor"
            ) {
                continue;
            }
            visit(&entry.path(), out, depth + 1)?;
        }
        Ok(())
    }
    let mut out = Vec::new();
    let base = if folder.is_empty() {
        root.to_path_buf()
    } else {
        contained(root, folder)?
    };
    visit(&base, &mut out, 0)?;
    out.sort();
    Ok(out)
}

pub fn json_file<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    serde_json::from_str(&text(path)?)
        .map_err(|e| format!("Invalid JSON in {}: {e}", path.display()))
}

pub fn write(root: &Path, name: &str, content: &str) -> Result<()> {
    let path = contained(root, name)?;
    fs::create_dir_all(path.parent().ok_or("Missing parent")?).map_err(|e| e.to_string())?;
    fs::write(path, content).map_err(|e| e.to_string())
}

pub fn pretty(value: &impl serde::Serialize) -> Result<String> {
    serde_json::to_string_pretty(value)
        .map(|s| s + "\n")
        .map_err(|e| e.to_string())
}

pub fn process(command: &mut Command, seconds: u64, limit: u64) -> Result<(bool, Vec<u8>)> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Cannot start command: {e}"))?;
    let stdout = child.stdout.take().ok_or("Missing stdout")?;
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = stdout
            .take(limit + 1)
            .read_to_end(&mut bytes)
            .map(|_| bytes);
        let _ = sender.send(result);
    });
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            break status;
        }
        if start.elapsed() > Duration::from_secs(seconds) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(
                "Command timed out; inspect the configured executable or dependency cache".into(),
            );
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let bytes = receiver
        .recv_timeout(Duration::from_secs(seconds).saturating_sub(start.elapsed()))
        .map_err(|_| "Command output timed out")?
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > limit {
        return Err("Command output exceeded the configured limit".into());
    }
    Ok((status.success(), bytes))
}

pub fn source_hash(root: &Path) -> Result<String> {
    let mut entries = BTreeMap::new();
    for name in ["src", "Cargo.toml", "build.rs"] {
        for path in files(root, name)? {
            entries.insert(
                slash(path.strip_prefix(root).map_err(|e| e.to_string())?),
                hash(text(&path)?.replace("\r\n", "\n")),
            );
        }
    }
    Ok(hash(
        serde_json::to_vec(&entries).map_err(|e| e.to_string())?,
    ))
}
