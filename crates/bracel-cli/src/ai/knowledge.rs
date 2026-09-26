use super::{project::Project, util::*};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[derive(Clone, Serialize, Deserialize)]
pub struct Capability {
    pub id: String,
    #[serde(rename = "crate")]
    pub package: String,
    pub status: String,
    pub api: String,
    pub source: String,
    pub docs: String,
    pub example: String,
    pub verification: String,
    #[serde(default)]
    pub feature: Option<String>,
    #[serde(default)]
    pub limitations: String,
}
#[derive(Deserialize)]
struct Catalog {
    schema_version: u32,
    capabilities: Vec<Capability>,
}

pub struct Document {
    pub origin: String,
    pub path: String,
    pub absolute: PathBuf,
    pub content: String,
}
pub struct Knowledge {
    pub documents: Vec<Document>,
    pub capabilities: Vec<Value>,
    pub guidelines: BTreeMap<String, String>,
    pub skills: BTreeMap<String, String>,
    pub hashes: BTreeMap<String, String>,
    pub warnings: Vec<String>,
    loaded_bytes: usize,
}

fn markdown(root: &Path, directory: &str) -> Result<BTreeMap<String, String>> {
    let base = contained(root, directory)?;
    let mut found = BTreeMap::new();
    for path in files(root, directory)? {
        if path.extension().is_some_and(|x| x == "md") {
            found.insert(
                slash(path.strip_prefix(&base).map_err(|e| e.to_string())?),
                text(&path)?,
            );
        }
    }
    Ok(found)
}

impl Knowledge {
    pub fn load(project: &Project) -> Result<Self> {
        let mut value = Self {
            documents: vec![],
            capabilities: vec![],
            guidelines: BTreeMap::new(),
            skills: BTreeMap::new(),
            hashes: BTreeMap::new(),
            warnings: vec![],
            loaded_bytes: 0,
        };
        let roots = if let Some(bundle) = &project.config.knowledge_bundle {
            let root = contained(&project.root, bundle)?;
            let manifest: Value = json_file(&contained(&root, "bundle.json")?)?;
            if manifest["schema_version"] != 1
                || manifest["packages"]
                    != serde_json::to_value(&project.packages).map_err(|e| e.to_string())?
            {
                return Err(
                    "Knowledge bundle does not match resolved framework sources/features".into(),
                );
            }
            for (name, digest) in manifest["files"]
                .as_object()
                .ok_or("Invalid bundle file manifest")?
            {
                if hash(text(&contained(&root, name)?)?) != digest.as_str().unwrap_or_default() {
                    return Err(format!("Knowledge bundle content mismatch: {name}"));
                }
            }
            for path in files(&root, "")? {
                let name = slash(path.strip_prefix(&root).map_err(|e| e.to_string())?);
                if name != "bundle.json" && manifest["files"].get(&name).is_none() {
                    return Err(format!("Unlisted knowledge bundle file: {name}"));
                }
            }
            vec![root]
        } else {
            project.roots()
        };
        for root in roots {
            let packages: Vec<_> = project
                .packages
                .iter()
                .filter(|p| {
                    project.config.knowledge_bundle.is_some() || p.directory.starts_with(&root)
                })
                .collect();
            let origin = packages
                .iter()
                .map(|p| {
                    format!(
                        "{}@{}:{}",
                        p.name,
                        p.version,
                        p.source.as_deref().unwrap_or("path")
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            let catalog_path = contained(&root, "ai/catalog.json")?;
            if catalog_path.exists() {
                let catalog: Catalog = json_file(&catalog_path)?;
                if catalog.schema_version != 1 {
                    return Err("Unsupported knowledge catalog schema; upgrade the companion or use a compatible bundle".into());
                }
                value.hashes.insert(
                    format!("{origin}/ai/catalog.json"),
                    hash(text(&catalog_path)?),
                );
                for capability in catalog.capabilities {
                    if !matches!(
                        capability.status.as_str(),
                        "implemented" | "optional" | "recipe" | "planned"
                    ) {
                        return Err(format!("Invalid capability status: {}", capability.id));
                    }
                    for reference in [
                        &capability.source,
                        &capability.docs,
                        &capability.example,
                        &capability.verification,
                    ] {
                        let path = contained(&root, reference)?;
                        if !path.is_file() {
                            return Err(format!("Missing capability reference: {reference}"));
                        }
                    }
                    let selected: Vec<_> = packages
                        .iter()
                        .filter(|p| p.name == capability.package)
                        .collect();
                    let available = !selected.is_empty()
                        && matches!(capability.status.as_str(), "implemented" | "optional")
                        && capability
                            .feature
                            .as_ref()
                            .is_none_or(|f| selected.iter().any(|p| p.features.contains(f)));
                    value.capabilities.push(json!({"definition":capability,"origin":origin,"available_in_resolution":available,"compiled":"not_checked","configured":"not_checked","verification":"not_checked"}));
                }
                // Fail on conflicting framework instructions instead of picking an
                // arbitrary version when multiple package sources coexist.
                for (name, content) in markdown(&root, "ai/guidelines")? {
                    if let Some(previous) = value.guidelines.insert(name.clone(), content.clone())
                        && previous != content
                    {
                        return Err(format!(
                            "Conflicting framework guideline {name}; select compatible dependency sources"
                        ));
                    }
                }
                for (name, content) in markdown(&root, "ai/skills")? {
                    if let Some(previous) = value.skills.insert(name.clone(), content.clone())
                        && previous != content
                    {
                        return Err(format!("Conflicting framework skill {name}"));
                    }
                }
            } else {
                value.warnings.push(format!(
                    "No knowledge catalog for {origin}; source-only search is available"
                ));
            }
            value.scan(
                &root,
                &origin,
                &[
                    "ai",
                    "docs",
                    "src",
                    "examples",
                    "tests",
                    "crates",
                    "starter/docs",
                    "starter/src",
                    "README.md",
                ],
            )?;
        }
        value.scan(
            &project.root,
            "application",
            &[
                "docs",
                "src",
                "examples",
                "tests",
                ".ai/guidelines",
                ".ai/skills",
                ".ai/rules",
                "README.md",
            ],
        )?;
        if project.application != project.root {
            value.scan(
                &project.application,
                "application-package",
                &["src", "docs", "tests", "README.md"],
            )?;
        }
        value
            .guidelines
            .extend(markdown(&project.root, ".ai/guidelines")?);
        value.skills.extend(markdown(&project.root, ".ai/skills")?);
        for (path, content) in &value.skills {
            if !path.ends_with("/SKILL.md")
                || path.split('/').count() != 2
                || !content.starts_with("---\n")
                || !content.contains("\nname:")
                || !content.contains("\ndescription:")
            {
                return Err(format!(
                    "Expected skill-name/SKILL.md with name and description frontmatter: {path}"
                ));
            }
        }
        Ok(value)
    }
    fn scan(&mut self, root: &Path, origin: &str, folders: &[&str]) -> Result<()> {
        for folder in folders {
            for path in files(root, folder)? {
                let relative = slash(path.strip_prefix(root).map_err(|e| e.to_string())?);
                if relative == ".ai/rules/index.md"
                    || !path
                        .extension()
                        .is_some_and(|x| matches!(x.to_str(), Some("md" | "rs" | "toml")))
                {
                    continue;
                }
                let content = text(&path)?.replace("\r\n", "\n");
                self.loaded_bytes += content.len();
                if self.loaded_bytes > 64_000_000 || self.documents.len() >= 20_000 {
                    return Err(
                        "Knowledge exceeds the 64 MB / 20,000 document request limit".into(),
                    );
                }
                self.hashes
                    .insert(format!("{origin}/{relative}"), hash(&content));
                self.documents.push(Document {
                    origin: origin.into(),
                    path: relative,
                    absolute: path,
                    content,
                });
            }
        }
        Ok(())
    }
    pub fn fingerprint(&self) -> Result<String> {
        Ok(hash(
            serde_json::to_vec(&self.hashes).map_err(|e| e.to_string())?,
        ))
    }
    pub fn search(&self, query: &str, limit: usize) -> Result<Value> {
        if query.trim().is_empty() || query.len() > 500 || !(1..=20).contains(&limit) {
            return Err("Search requires a query of 1..500 bytes and limit of 1..20".into());
        }
        let terms: Vec<_> = query.split_whitespace().map(str::to_lowercase).collect();
        let mut results = Vec::new();
        for doc in &self.documents {
            let lines: Vec<_> = doc.content.lines().collect();
            for (block, chunk) in lines.chunks(24).enumerate() {
                let excerpt = chunk.join("\n");
                let lower = excerpt.to_lowercase();
                let score = terms.iter().filter(|t| lower.contains(t.as_str())).count();
                if score == 0 {
                    continue;
                }
                let heading = lines[..(block * 24 + chunk.len())]
                    .iter()
                    .rev()
                    .find(|l| l.starts_with('#') || l.starts_with("pub "))
                    .copied()
                    .unwrap_or(&doc.path);
                results.push((score,json!({"origin":doc.origin,"path":doc.path,"absolute_path":slash(&doc.absolute),"line":block*24+1,"heading":heading,"content_hash":hash(&doc.content),"excerpt":excerpt.chars().take(1600).collect::<String>(),"kind":if doc.path.contains("research/") || doc.path.contains("adr/") {"design_reference"} else {"source"}})));
            }
        }
        results.sort_by_key(|a| std::cmp::Reverse(a.0));
        Ok(
            json!({"query":query,"results":results.into_iter().take(limit).map(|(_,v)|v).collect::<Vec<_>>(),"knowledge_hash":self.fingerprint()?,"warnings":self.warnings}),
        )
    }
}

pub fn rule_index(root: &Path) -> Result<String> {
    let mut output = String::from(
        "# Application rules\n\nBefore editing, read every rule whose paths match the files you will change.\n\n| Paths | Rule |\n| --- | --- |\n",
    );
    for (name, content) in markdown(root, ".ai/rules")? {
        if name == "index.md" {
            continue;
        }
        let header = content
            .strip_prefix("---\n")
            .and_then(|s| s.split_once("\n---").map(|(h, _)| h))
            .ok_or_else(|| format!("Rule {name} needs paths frontmatter"))?;
        let mut globs = Vec::new();
        let mut in_paths = false;
        for line in header.lines() {
            if line == "paths:" {
                in_paths = true;
                continue;
            }
            if in_paths && let Some(glob) = line.trim().strip_prefix("- ") {
                globs.push(glob.trim_matches(['\'', '"']).replace('|', "\\|"));
            } else if !line.starts_with(' ') {
                in_paths = false;
            }
        }
        if globs.is_empty() {
            return Err(format!("Rule {name} requires a nonempty YAML paths list"));
        }
        output.push_str(&format!(
            "| {} | [.ai/rules/{name}](./{name}) |\n",
            globs.join(", ")
        ));
    }
    Ok(output)
}
