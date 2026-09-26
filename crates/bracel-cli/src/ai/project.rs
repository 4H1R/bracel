use super::util::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub schema_version: u32,
    pub agents: Vec<String>,
    pub manifest: String,
    #[serde(default)]
    pub features: Vec<String>,
    #[serde(default)]
    pub no_default_features: bool,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub inspection_executable: Option<String>,
    #[serde(default)]
    pub knowledge_bundle: Option<String>,
    #[serde(default = "timeout")]
    pub inspection_timeout_seconds: u64,
}
fn timeout() -> u64 {
    10
}
impl Default for Config {
    fn default() -> Self {
        Self {
            schema_version: 1,
            agents: vec!["codex".into()],
            manifest: "Cargo.toml".into(),
            features: vec![],
            no_default_features: false,
            target: None,
            inspection_executable: None,
            knowledge_bundle: None,
            inspection_timeout_seconds: timeout(),
        }
    }
}
impl Config {
    pub fn load(root: &Path) -> Result<Self> {
        let path = contained(root, ".bracel/ai.json")?;
        let config: Self = if path.exists() {
            json_file(&path)?
        } else {
            Self::default()
        };
        config.validate()?;
        Ok(config)
    }
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != 1 {
            return Err("Unsupported AI configuration schema".into());
        }
        relative(Path::new(&self.manifest))?;
        if self.agents.is_empty()
            || self
                .agents
                .iter()
                .any(|a| !matches!(a.as_str(), "codex" | "claude" | "cursor"))
        {
            return Err("Agents must be codex, claude or cursor".into());
        }
        if self.inspection_timeout_seconds == 0 || self.inspection_timeout_seconds > 60 {
            return Err("Inspection timeout must be 1..60 seconds".into());
        }
        Ok(())
    }
}

#[derive(Clone, Serialize)]
pub struct Package {
    pub name: String,
    pub version: String,
    pub source: Option<String>,
    #[serde(rename = "resolved_features")]
    pub features: Vec<String>,
    pub source_hash: String,
    #[serde(skip)]
    pub directory: PathBuf,
}

pub struct Project {
    pub root: PathBuf,
    pub application: PathBuf,
    pub config: Config,
    pub packages: Vec<Package>,
    pub lock_hash: String,
    pub application_hash: String,
    pub name: String,
}

impl Project {
    pub fn resolve(root: &Path, config: Config) -> Result<Self> {
        config.validate()?;
        let manifest = contained(root, &config.manifest)?;
        let mut command = Command::new("cargo");
        command
            .current_dir(root)
            .args([
                "metadata",
                "--format-version",
                "1",
                "--locked",
                "--offline",
                "--manifest-path",
            ])
            .arg(&manifest);
        if !config.features.is_empty() {
            command.arg("--features").arg(config.features.join(","));
        }
        if config.no_default_features {
            command.arg("--no-default-features");
        }
        if let Some(target) = &config.target {
            command.arg("--filter-platform").arg(target);
        }
        let (ok, data) = process(&mut command, 60, 64_000_000)?;
        if !ok {
            return Err("DEPENDENCIES.UNRESOLVED: cargo metadata --locked --offline failed. Fetch dependencies or update the lockfile explicitly, and check the configured manifest/features/target.".into());
        }
        let metadata: Value = serde_json::from_slice(&data).map_err(|e| e.to_string())?;
        let packages = metadata["packages"]
            .as_array()
            .ok_or("Missing Cargo packages")?;
        let resolved = &metadata["resolve"];
        let selected = resolved["root"].as_str().ok_or(
            "Select an application with --manifest-path; a virtual workspace has no root package",
        )?;
        let app = packages
            .iter()
            .find(|p| p["id"] == selected)
            .ok_or("Missing selected package")?;
        let application = PathBuf::from(
            app["manifest_path"]
                .as_str()
                .ok_or("Missing application path")?,
        )
        .parent()
        .ok_or("Missing parent")?
        .canonicalize()
        .map_err(|e| e.to_string())?;
        let nodes = resolved["nodes"]
            .as_array()
            .ok_or("Missing Cargo dependency graph")?;
        let mut seen = BTreeSet::new();
        let mut pending = vec![selected.to_string()];
        while let Some(id) = pending.pop() {
            if !seen.insert(id.clone()) {
                continue;
            }
            if let Some(node) = nodes.iter().find(|n| n["id"] == id) {
                for dep in node["dependencies"]
                    .as_array()
                    .ok_or("Invalid dependency graph")?
                {
                    pending.push(dep.as_str().ok_or("Invalid package identity")?.into());
                }
            }
        }
        let mut found = Vec::new();
        for p in packages {
            let name = p["name"].as_str().ok_or("Missing package name")?;
            if !(name == "bracel" || name.starts_with("bracel-"))
                || name == "bracel-starter"
                || name == "bracel-cli"
                || !seen.contains(p["id"].as_str().unwrap_or_default())
            {
                continue;
            }
            let directory =
                PathBuf::from(p["manifest_path"].as_str().ok_or("Missing package path")?)
                    .parent()
                    .ok_or("Missing parent")?
                    .canonicalize()
                    .map_err(|e| e.to_string())?;
            let node = nodes
                .iter()
                .find(|n| n["id"] == p["id"])
                .ok_or("Missing resolved node")?;
            let features = node["features"]
                .as_array()
                .ok_or("Missing features")?
                .iter()
                .filter_map(Value::as_str)
                .map(String::from)
                .collect();
            found.push(Package {
                name: name.into(),
                version: p["version"].as_str().unwrap_or_default().into(),
                source: p["source"].as_str().map(String::from),
                features,
                source_hash: source_hash(&directory)?,
                directory,
            });
        }
        found.sort_by(|a, b| {
            (&a.name, &a.version, &a.source).cmp(&(&b.name, &b.version, &b.source))
        });
        if found.is_empty() {
            return Err(
                "No resolved Bracel framework dependency in the selected application".into(),
            );
        }
        let workspace = Path::new(
            metadata["workspace_root"]
                .as_str()
                .ok_or("Missing workspace root")?,
        );
        Ok(Self {
            root: root.to_path_buf(),
            config,
            application_hash: source_hash(&application)?,
            application,
            packages: found,
            lock_hash: hash(text(&workspace.join("Cargo.lock"))?.replace("\r\n", "\n")),
            name: app["name"].as_str().unwrap_or_default().into(),
        })
    }
    pub fn identity(&self) -> Value {
        json!({"application":self.name,"application_hash":self.application_hash,"packages":self.packages,"lock_hash":self.lock_hash,"features":self.config.features,"target":self.config.target,"no_default_features":self.config.no_default_features})
    }
    pub fn roots(&self) -> Vec<PathBuf> {
        let mut roots = BTreeSet::new();
        for package in &self.packages {
            // A Git checkout/workspace may put ai/ at its root. Stop at the
            // nearest repository boundary instead of walking into a user's home.
            let mut selected = package.directory.clone();
            for ancestor in package.directory.ancestors().take(4) {
                if ancestor.join("ai/catalog.json").is_file() {
                    selected = ancestor.to_path_buf();
                    break;
                }
                if ancestor.join(".git").exists() {
                    break;
                }
            }
            roots.insert(selected);
        }
        roots.into_iter().collect()
    }
    pub fn provenance(&self) -> Value {
        let source_roots: BTreeMap<_, _> = self
            .packages
            .iter()
            .map(|p| {
                (
                    format!(
                        "{}@{}:{}",
                        p.name,
                        p.version,
                        p.source.as_deref().unwrap_or("path")
                    ),
                    slash(&p.directory),
                )
            })
            .collect();
        json!({"schema_version":1,"identity":self.identity(),"source_roots":source_roots,"knowledge_mode":"resolved_source","compiled_state":"not_checked"})
    }
}
