use super::{project::Project, util::*};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path, process::Command};

pub fn inspect(project: &Project) -> Result<Value> {
    let Some(executable) = &project.config.inspection_executable else {
        return Ok(
            json!({"schema_version":1,"ok":false,"code":"INSPECTION.NOT_CONFIGURED","compiled_state":"not_checked","remediation":"Set inspection_executable in .bracel/ai.json to the trusted application binary. Only inspect --json will be called."}),
        );
    };
    let executable = if Path::new(executable).is_absolute() {
        executable.into()
    } else {
        contained(&project.root, executable)?
    };
    let (success, bytes) = process(
        Command::new(executable)
            .current_dir(&project.application)
            .args(["inspect", "--json"]),
        project.config.inspection_timeout_seconds,
        2_000_000,
    )?;
    let report: Value = serde_json::from_slice(&bytes)
        .map_err(|_| "Inspection executable did not return valid bounded JSON")?;
    if report["schema_version"] != 1 || report["command"] != "inspect" || !report["ok"].is_boolean()
    {
        return Err("Unsupported application inspection contract".into());
    }
    let provenance = &report["application"]["build_provenance"];
    let compiled_state = if provenance.is_null() {
        "unknown"
    } else if provenance["application_hash"] != project.application_hash
        || provenance["lock_hash"] != project.lock_hash
    {
        "stale"
    } else {
        let changed = project
            .packages
            .iter()
            .filter(|p| p.source.is_none())
            .any(|p| provenance["path_dependencies"][&p.name] != p.source_hash);
        let requested = &project.config.features;
        let compiled = provenance["features"].as_array();
        if changed
            || compiled.is_none()
            || requested
                .iter()
                .any(|f| !compiled.unwrap().iter().any(|v| v == f))
            || project
                .config
                .target
                .as_ref()
                .is_some_and(|t| provenance["target"] != *t)
        {
            "stale"
        } else {
            "source_matches; inspect compiled capability fields"
        }
    };
    // Expose only the defined inspection envelope, not subprocess stderr.
    Ok(
        json!({"schema_version":1,"ok":success && report["ok"]==true && !matches!(compiled_state,"stale"|"unknown"),"compiled_state":compiled_state,"report":report}),
    )
}

pub fn bundle(project: &Project, output: &Path) -> Result<Value> {
    let roots = project.roots();
    if roots.len() != 1 {
        return Err(
            "Bundle creation requires one framework source root; bundle each source independently"
                .into(),
        );
    }
    if output.exists() {
        return Err("Bundle destination must not exist".into());
    }
    let source = &roots[0];
    let mut content = BTreeMap::new();
    for folder in [
        "ai",
        "docs",
        "src",
        "examples",
        "tests",
        "scripts",
        "crates",
        "starter/docs",
        "starter/src",
        "starter/tests",
        "README.md",
        "Cargo.toml",
    ] {
        for path in files(source, folder)? {
            if path.extension().is_some_and(|ext| {
                matches!(
                    ext.to_str(),
                    Some("md" | "rs" | "json" | "toml" | "sh" | "py")
                )
            }) {
                let name = slash(path.strip_prefix(source).map_err(|e| e.to_string())?);
                content.insert(name, text(&path)?.replace("\r\n", "\n"));
            }
        }
    }
    if !content.contains_key("ai/catalog.json") {
        return Err("Source has no knowledge catalog to bundle".into());
    }
    let hashes: BTreeMap<_, _> = content
        .iter()
        .map(|(name, body)| (name.clone(), hash(body)))
        .collect();
    // Gather and validate before creating the destination.
    for (name, body) in content {
        write(output, &name, &body)?;
    }
    write(
        output,
        "bundle.json",
        &pretty(&json!({"schema_version":1,"packages":project.packages,"files":hashes}))?,
    )?;
    Ok(json!({"schema_version":1,"ok":true,"output":slash(output),"files":hashes.len()}))
}
