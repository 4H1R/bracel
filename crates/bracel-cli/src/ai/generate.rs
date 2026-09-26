use super::{
    knowledge::{Knowledge, rule_index},
    project::Project,
    util::*,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::Path};

const BEGIN: &str = "<!-- bracel:begin -->";
const END: &str = "<!-- bracel:end -->";
#[derive(Clone, Serialize, Deserialize)]
pub struct Owned {
    mode: String,
    hash: String,
}
#[derive(Serialize, Deserialize)]
pub struct Lock {
    pub schema_version: u32,
    generator: String,
    pub identity: Value,
    pub knowledge_hash: String,
    config_hash: String,
    owned: BTreeMap<String, Owned>,
}
pub fn lock(root: &Path) -> Result<Option<Lock>> {
    let path = contained(root, ".bracel/ai.lock.json")?;
    if !path.exists() {
        return Ok(None);
    }
    let value: Lock = json_file(&path)?;
    if value.schema_version != 1 {
        return Err("Unsupported generation manifest schema".into());
    }
    Ok(Some(value))
}

fn section(content: &str) -> Result<Option<&str>> {
    if !content.contains(BEGIN) && !content.contains(END) {
        return Ok(None);
    }
    if content.matches(BEGIN).count() != 1 || content.matches(END).count() != 1 {
        return Err("Invalid or duplicate Bracel instruction markers".into());
    }
    let start = content.find(BEGIN).ok_or("Missing marker")?;
    let end = content.find(END).ok_or("Missing marker")? + END.len();
    if start >= end {
        return Err("Invalid Bracel marker order".into());
    }
    Ok(Some(&content[start..end]))
}

fn part(content: &str, mode: &str) -> Result<Option<String>> {
    match mode {
        "block" => Ok(section(content)?.map(String::from)),
        "toml" => {
            if content.is_empty() {
                return Ok(None);
            }
            let doc = content
                .parse::<toml_edit::DocumentMut>()
                .map_err(|e| e.to_string())?;
            Ok(doc
                .get("mcp_servers")
                .and_then(|x| x.get("bracel"))
                .map(|v| v.to_string()))
        }
        "json" => {
            if content.is_empty() {
                return Ok(None);
            }
            let doc: Value = serde_json::from_str(content).map_err(|e| e.to_string())?;
            doc.get("mcpServers")
                .and_then(|v| v.get("bracel"))
                .map(pretty)
                .transpose()
        }
        "file" => Ok((!content.is_empty()).then(|| content.into())),
        _ => Err("Unknown generated file mode".into()),
    }
}
fn replace(content: &str, mode: &str, replacement: Option<&str>) -> Result<String> {
    match mode {
        "block" => {
            if let Some(old) = section(content)? {
                Ok(content.replacen(old, replacement.unwrap_or(""), 1))
            } else {
                Ok(format!(
                    "{}{separator}{}\n",
                    content,
                    replacement.unwrap_or(""),
                    separator = if content.is_empty() { "" } else { "\n" }
                ))
            }
        }
        "toml" => {
            let mut doc = content
                .parse::<toml_edit::DocumentMut>()
                .map_err(|e| e.to_string())?;
            if let Some(replacement) = replacement {
                let parsed = replacement
                    .parse::<toml_edit::DocumentMut>()
                    .map_err(|e| e.to_string())?;
                if doc.get("mcp_servers").is_none() {
                    doc["mcp_servers"] = toml_edit::Item::Table(toml_edit::Table::new());
                }
                if !doc["mcp_servers"].is_table() {
                    return Err("mcp_servers must be a TOML table".into());
                }
                doc["mcp_servers"]["bracel"] = parsed["mcp_servers"]["bracel"].clone();
            } else if let Some(table) = doc
                .get_mut("mcp_servers")
                .and_then(toml_edit::Item::as_table_mut)
            {
                table.remove("bracel");
            }
            Ok(doc.to_string())
        }
        "json" => {
            let mut doc: Value = if content.trim().is_empty() {
                json!({})
            } else {
                serde_json::from_str(content).map_err(|e| e.to_string())?
            };
            if !doc.is_object() {
                return Err("MCP configuration must be a JSON object".into());
            }
            if doc.get("mcpServers").is_none() {
                doc["mcpServers"] = json!({});
            }
            let servers = doc["mcpServers"]
                .as_object_mut()
                .ok_or("mcpServers must be an object")?;
            if let Some(value) = replacement {
                servers.insert(
                    "bracel".into(),
                    serde_json::from_str(value).map_err(|e| e.to_string())?,
                );
            } else {
                servers.remove("bracel");
            }
            pretty(&doc)
        }
        "file" => Ok(replacement.unwrap_or("").into()),
        _ => Err("Unknown generated file mode".into()),
    }
}

fn desired(project: &Project, knowledge: &Knowledge) -> Result<BTreeMap<String, (String, String)>> {
    let mut files = BTreeMap::new();
    let mut guidance = String::from(
        "# Bracel project context\n\nAt task start call `project_info` (or `bracel ai info`). Search version-matched documentation with `search_docs` before unfamiliar API work. Read current source and tests before editing. Run `bracel ai doctor` to identify stale context. Read matching application rules in `.ai/rules/index.md`. Reload the agent session after updating instructions or skills.\n\nResolved framework dependencies:\n",
    );
    for p in &project.packages {
        guidance.push_str(&format!(
            "- {} {} ({})\n",
            p.name,
            p.version,
            p.source
                .as_deref()
                .unwrap_or("local source; inspect working tree")
        ));
    }
    for body in knowledge.guidelines.values() {
        guidance.push('\n');
        guidance.push_str(body);
    }
    for warning in &knowledge.warnings {
        guidance.push_str(&format!("\nKnowledge limitation: {warning}\n"));
    }
    let block = format!("{BEGIN}\n{guidance}\n{END}");
    for agent in &project.config.agents {
        let (instructions, skills, mcp, mode) = match agent.as_str() {
            "codex" => ("AGENTS.md", ".agents/skills", ".codex/config.toml", "toml"),
            "claude" => ("CLAUDE.md", ".claude/skills", ".mcp.json", "json"),
            "cursor" => (
                ".cursor/rules/bracel.mdc",
                ".cursor/skills",
                ".cursor/mcp.json",
                "json",
            ),
            _ => return Err("Unsupported agent".into()),
        };
        if agent == "cursor" {
            files.insert(instructions.into(),("file".into(),format!("---\ndescription: Bracel framework context\nalwaysApply: true\n---\n{guidance}")));
        } else {
            files.insert(instructions.into(), ("block".into(), block.clone()));
        }
        for (name, body) in &knowledge.skills {
            files.insert(format!("{skills}/{name}"), ("file".into(), body.clone()));
        }
        let server = if mode == "toml" {
            "[mcp_servers.bracel]\ncommand = \"bracel\"\nargs = [\"ai\", \"mcp\"]\n".into()
        } else {
            pretty(&json!({"command":"bracel","args":["ai","mcp"]}))?
        };
        files.insert(mcp.into(), (mode.into(), server));
    }
    files.insert(
        ".ai/rules/index.md".into(),
        ("file".into(), rule_index(&project.root)?),
    );
    Ok(files)
}

pub fn sync(project: &Project, knowledge: &Knowledge, check: bool, dry: bool) -> Result<Value> {
    let old = lock(&project.root)?;
    let mut wanted = desired(project, knowledge)?;
    if let Some(old) = &old {
        for (path, owned) in &old.owned {
            wanted
                .entry(path.clone())
                .or_insert((owned.mode.clone(), String::new()));
        }
    }
    let mut writes = BTreeMap::new();
    let mut owned = BTreeMap::new();
    for (name, (mode, body)) in wanted {
        let path = contained(&project.root, &name)?;
        let current = if path.exists() {
            text(&path)?
        } else {
            String::new()
        };
        let existing = part(&current, &mode)?;
        let replacement = (!body.is_empty()).then_some(body.as_str());
        let next = replace(&current, &mode, replacement)?;
        let next_part = part(&next, &mode)?;
        if let Some(existing) = &existing {
            let tracked = old.as_ref().and_then(|o| o.owned.get(&name));
            if tracked.is_some_and(|o| o.mode != mode || o.hash != hash(existing))
                || (tracked.is_none() && existing.as_str() != next_part.as_deref().unwrap_or(""))
            {
                return Err(format!(
                    "GENERATION.CONFLICT: {name} contains edits outside the last generated snapshot. Preserve them in .ai/guidelines or .ai/skills, inspect git diff -- {name}, then restore only the generated section before syncing."
                ));
            }
        }
        if let Some(value) = next_part {
            owned.insert(
                name.clone(),
                Owned {
                    mode: mode.clone(),
                    hash: hash(value),
                },
            );
        }
        if next != current {
            writes.insert(name, next);
        }
    }
    let next_lock = Lock {
        schema_version: 1,
        generator: env!("CARGO_PKG_VERSION").into(),
        identity: project.identity(),
        knowledge_hash: knowledge.fingerprint()?,
        config_hash: hash(pretty(&project.config)?),
        owned,
    };
    let lock_content = pretty(&next_lock)?;
    let lock_path = contained(&project.root, ".bracel/ai.lock.json")?;
    if !lock_path.exists() || text(&lock_path)? != lock_content {
        writes.insert(".bracel/ai.lock.json".into(), lock_content);
    }
    let config_content = pretty(&project.config)?;
    let config_path = contained(&project.root, ".bracel/ai.json")?;
    if !config_path.exists() || text(&config_path)? != config_content {
        writes.insert(".bracel/ai.json".into(), config_content);
    }
    let changes: Vec<_> = writes.keys().cloned().collect();
    if !check && !dry {
        // Preflight all conflicts before any mutation; write ownership manifest last.
        for (name, content) in writes
            .iter()
            .filter(|(name, _)| name.as_str() != ".bracel/ai.lock.json")
        {
            if content.is_empty() {
                let path = contained(&project.root, name)?;
                if path.exists() {
                    fs::remove_file(path).map_err(|e| e.to_string())?;
                }
            } else {
                write(&project.root, name, content)?;
            }
        }
        if let Some(content) = writes.get(".bracel/ai.lock.json") {
            write(&project.root, ".bracel/ai.lock.json", content)?;
        }
    }
    Ok(
        json!({"schema_version":1,"ok":!check || changes.is_empty(),"status":if check && !changes.is_empty(){"stale"}else if dry{"preview"}else{"current"},"changes":changes,"warnings":knowledge.warnings}),
    )
}

pub fn doctor(project: &Project, knowledge: &Knowledge) -> Value {
    match sync(project, knowledge, true, false) {
        Ok(result) => {
            json!({"schema_version":1,"ok":result["ok"],"checks":[{"code":if result["ok"]==true {"AI.CURRENT"}else{"AI.STALE"},"remediation":"Run bracel ai sync after changing source, docs, dependencies or rules; reload agent instructions if changed."}],"generation":result,"project":project.provenance(),"compiled_state":"not_checked"})
        }
        Err(message) => {
            json!({"schema_version":1,"ok":false,"checks":[{"code":"AI.CONFLICT","message":message}],"project":project.provenance()})
        }
    }
}
