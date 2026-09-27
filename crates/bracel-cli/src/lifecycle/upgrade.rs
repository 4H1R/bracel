use super::*;
use serde_json::json;
use toml_edit::{DocumentMut, Item, TableLike, value};

fn framework(name: &str) -> bool {
    matches!(
        name,
        "bracel"
            | "bracel-integrations"
            | "bracel-jobs"
            | "bracel-data"
            | "bracel-delivery"
            | "bracel-files"
            | "bracel-realtime"
    )
}

fn dependencies(
    table: &mut dyn TableLike,
    release: &Path,
    selected: &str,
    revision: &str,
    changes: &mut Vec<serde_json::Value>,
) -> Result<()> {
    for (alias, dependency) in table.iter_mut() {
        let alias = alias.get().to_string();
        let name = dependency
            .get("package")
            .and_then(Item::as_str)
            .unwrap_or(&alias)
            .to_string();
        if !framework(&name) {
            continue;
        }
        let dependency = dependency.as_table_like_mut().ok_or("Expected explicit Git Bracel dependencies; registry, workspace-inherited and path dependencies require manual adoption.")?;
        let source = dependency.get("git").and_then(Item::as_str).ok_or("Only standalone Git-distributed applications can be upgraded automatically. Path/workspace dependencies require manual updates.")?;
        if source.trim_end_matches('/').trim_end_matches(".git") != FRAMEWORK
            || dependency.contains_key("path")
            || dependency.contains_key("workspace")
        {
            return Err("Custom framework sources require a manual upgrade.".into());
        }
        let manifest: toml::Value =
            toml::from_str(&read(&release.join(format!("crates/{name}/Cargo.toml")))?)
                .map_err(|e| e.to_string())?;
        if manifest
            .get("package")
            .and_then(|p| p.get("version"))
            .and_then(|v| v.as_str())
            != Some(selected)
        {
            return Err(format!(
                "The release does not contain {name} {selected}; no application files were changed."
            ));
        }
        let previous = dependency
            .get("rev")
            .and_then(Item::as_str)
            .unwrap_or("unpinned");
        if previous != revision
            || dependency.get("version").and_then(Item::as_str) != Some(selected)
        {
            changes.push(json!({"dependency":name,"alias":alias,"from_revision":previous,"to_version":selected,"to_revision":revision}));
        }
        dependency.remove("branch");
        dependency.remove("tag");
        dependency.insert("version", value(selected));
        dependency.insert("rev", value(revision));
    }
    Ok(())
}

fn update_tables(
    doc: &mut dyn TableLike,
    source: &Path,
    selected: &str,
    revision: &str,
    changes: &mut Vec<serde_json::Value>,
) -> Result<()> {
    for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
        if let Some(table) = doc.get_mut(section).and_then(Item::as_table_like_mut) {
            dependencies(table, source, selected, revision, changes)?;
        }
    }
    if let Some(targets) = doc.get_mut("target").and_then(Item::as_table_like_mut) {
        for (_, target) in targets.iter_mut() {
            if let Some(target) = target.as_table_like_mut() {
                update_tables(target, source, selected, revision, changes)?;
            }
        }
    }
    Ok(())
}

pub fn run(root: &Path, mode: Option<Mode>, args: &[String]) -> Result<()> {
    let mut check = false;
    let mut requested = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--check" if !check => check = true,
            "--to" if requested.is_none() => {
                index += 1;
                requested = Some(args.get(index).ok_or("Missing --to version")?.as_str());
            }
            _ => return Err(crate::USAGE.into()),
        }
        index += 1;
    }
    let manifest_path = root.join("Cargo.toml");
    read(&root.join("Cargo.lock"))?;
    let original_manifest = fs::read(&manifest_path)
        .map_err(|_| "Run bracel upgrade in an application root".to_string())?;
    let original_lock = fs::read(root.join("Cargo.lock"))
        .map_err(|_| "Commit a Cargo.lock before upgrading".to_string())?;
    let mut doc = read(&manifest_path)?
        .parse::<DocumentMut>()
        .map_err(|e| e.to_string())?;
    if doc.get("package").is_none()
        || doc
            .get("workspace")
            .and_then(|v| v.get("members"))
            .is_some()
        || doc.contains_key("patch")
        || doc.contains_key("replace")
    {
        return Err("Automatic upgrade supports standalone applications without workspace members or dependency patches. Update this workspace manually.".into());
    }
    if !check {
        let clean = output(
            Command::new("git").current_dir(root).args([
                "status",
                "--porcelain",
                "--untracked-files=normal",
            ]),
            "Check application working tree",
        )?;
        if !clean.trim().is_empty() {
            return Err("Commit or stash your application changes before upgrading. bracel upgrade --check can preview with a dirty working tree.".into());
        }
    }
    let selected = release::select(requested)?;
    let (_temporary, source, revision) = release::checkout(&selected)?;
    let mut changes = Vec::new();
    update_tables(
        doc.as_table_mut(),
        &source,
        &selected,
        &revision,
        &mut changes,
    )?;
    if changes.is_empty() {
        println!(
            "No dependency changes. The application is already pinned to {selected}, or has no supported Bracel dependencies."
        );
        return Ok(());
    }
    let toolchain = read(&source.join("rust-toolchain.toml"))?;
    let original_toolchain = read(&root.join("rust-toolchain.toml"))?;
    let mut project_toolchain = original_toolchain
        .parse::<DocumentMut>()
        .map_err(|e| e.to_string())?;
    let release_toolchain = toolchain
        .parse::<DocumentMut>()
        .map_err(|e| e.to_string())?;
    let current_channel = project_toolchain["toolchain"]["channel"]
        .as_str()
        .ok_or("Missing application toolchain channel")?
        .to_string();
    let target_channel = release_toolchain["toolchain"]["channel"]
        .as_str()
        .ok_or("Missing release toolchain channel")?;
    let toolchain_change = release::version(target_channel)? > release::version(&current_channel)?;
    if toolchain_change {
        project_toolchain["toolchain"]["channel"] = value(target_channel);
    }
    println!("{}", serde_json::to_string_pretty(&json!({"schema_version":1,"target_version":selected,"target_revision":revision,
        "changes":changes,"release_notes":format!("{FRAMEWORK}/blob/{revision}/CHANGELOG.md"),
        "recommended_toolchain":toolchain,"toolchain_change":if toolchain_change {Some(json!({"from":current_channel,"to":target_channel}))} else {None},
        "application_changes":"Review release notes and starter changes for configuration/API changes and forward migrations. Application files and migration history are preserved."})).map_err(|e| e.to_string())?);
    if check {
        println!("Preview only. To apply: bracel upgrade --to {selected}");
        return Ok(());
    }
    let mode = mode.unwrap_or(project::Settings::load(root)?.mode);
    // Snapshot managed files before any Cargo process. Restore byte-for-byte if
    // resolution or compilation fails; no Git reset and no migration execution.
    let updated = doc.to_string();
    write(&manifest_path, &updated)?;
    let result = (|| {
        if toolchain_change {
            write(
                &root.join("rust-toolchain.toml"),
                project_toolchain.to_string(),
            )?;
        }
        // Cargo resolves only changes required by the edited requirements while
        // preserving the other locked packages. The one check compiles all app targets.
        project::cargo(root, mode, &["check".into(), "--all-targets".into()])
    })();
    if let Err(error) = result {
        write(&manifest_path, &original_manifest)?;
        write(&root.join("Cargo.lock"), &original_lock)?;
        if toolchain_change {
            write(&root.join("rust-toolchain.toml"), &original_toolchain)?;
        }
        return Err(format!(
            "{error}\nRestored dependency and toolchain files. Review the release notes before retrying."
        ));
    }
    println!(
        "Framework dependencies upgraded to {selected}; Cargo checks passed.\nReview the Git diff and release notes, run your application's tests, and commit the upgrade. Database migrations were not run."
    );
    if toolchain_change {
        println!(
            "Rust channel updated to {target_channel}. Align your deployment Dockerfile's Rust builder and any CI compiler pins with this version."
        );
    }
    if root.join(".bracel/ai.json").exists() {
        if mode == Mode::Native {
            let executable = std::env::current_exe().map_err(|e| e.to_string())?;
            let status = Command::new(executable)
                .current_dir(root)
                .args(["ai", "sync"])
                .status()
                .map_err(|e| e.to_string())?;
            if !status.success() {
                return Err("Dependencies upgraded, but AI context needs attention. Resolve the reported conflict and run bracel ai sync; the successful dependency upgrade was retained.".into());
            }
            println!("AI context refreshed. Reload your coding agent.");
        } else {
            project::docker_ai(root, &["sync".into()]).map_err(|error| format!("Dependencies upgraded, but AI sync failed: {error}\nRun bracel ai sync after resolving the reported issue."))?;
            println!("AI context refreshed in Docker. Reload your coding agent.");
        }
    }
    Ok(())
}
