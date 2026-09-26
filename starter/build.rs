use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
};

fn digest(bytes: impl AsRef<[u8]>) -> String {
    format!("{:x}", Sha256::digest(bytes.as_ref()))
}

fn source_hash(root: &Path) -> String {
    fn visit(root: &Path, path: &Path, entries: &mut BTreeMap<String, String>) {
        if !path.exists() {
            return;
        }
        let meta = fs::symlink_metadata(path).expect("source metadata");
        if meta.file_type().is_symlink() {
            return;
        }
        if meta.is_dir() {
            for entry in fs::read_dir(path).expect("source directory") {
                visit(root, &entry.expect("source entry").path(), entries);
            }
        } else {
            entries.insert(
                path.strip_prefix(root)
                    .expect("source path")
                    .to_string_lossy()
                    .replace('\\', "/"),
                digest(
                    fs::read_to_string(path)
                        .expect("source content")
                        .replace("\r\n", "\n"),
                ),
            );
        }
    }
    let mut entries = BTreeMap::new();
    for name in ["src", "Cargo.toml", "build.rs"] {
        let path = root.join(name);
        println!("cargo:rerun-if-changed={}", path.display());
        visit(root, &path, &mut entries);
    }
    digest(serde_json::to_vec(&entries).expect("source manifest"))
}

fn collect_path_dependencies(root: &Path, found: &mut BTreeMap<String, String>) {
    let manifest: toml::Value =
        toml::from_str(&fs::read_to_string(root.join("Cargo.toml")).expect("dependency manifest"))
            .expect("dependency TOML");
    for section in ["dependencies", "dev-dependencies"] {
        if let Some(deps) = manifest.get(section).and_then(toml::Value::as_table) {
            for (alias, dep) in deps {
                let name = dep
                    .get("package")
                    .and_then(toml::Value::as_str)
                    .unwrap_or(alias);
                if (name == "bracel" || name.starts_with("bracel-"))
                    && !found.contains_key(name)
                    && let Some(path) = dep.get("path").and_then(toml::Value::as_str)
                {
                    let directory = root.join(path);
                    found.insert(name.into(), source_hash(&directory));
                    collect_path_dependencies(&directory, found);
                }
            }
        }
    }
}

fn main() {
    println!("cargo:rerun-if-changed=Cargo.lock");
    println!("cargo:rerun-if-changed=../Cargo.lock");
    let lock = fs::read_to_string("Cargo.lock")
        .or_else(|_| fs::read_to_string("../Cargo.lock"))
        .expect("read application lockfile");
    let packages = [
        "bracel",
        "axum",
        "sea-orm",
        "sea-orm-migration",
        "tokio",
        "utoipa",
        "uuid",
        "jsonwebtoken",
        "governor",
        "tower-http",
    ];
    let mut entries = Vec::new();
    for package in packages {
        let mut versions = Vec::new();
        for block in lock.split("[[package]]").skip(1) {
            let field = |key: &str| {
                block.lines().find_map(|line| {
                    line.strip_prefix(&format!("{key} = \""))
                        .and_then(|value| value.strip_suffix('"'))
                })
            };
            if field("name") == Some(package) {
                let version = field("version").expect("locked package version");
                assert!(
                    version
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || ".-+".contains(c))
                );
                versions.push(format!("\"{version}\""));
            }
        }
        assert!(!versions.is_empty(), "missing locked package: {package}");
        entries.push(format!("\"{package}\":[{}]", versions.join(",")));
    }
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest directory"));
    let mut path_dependencies = BTreeMap::new();
    collect_path_dependencies(&root, &mut path_dependencies);
    let mut features: Vec<_> = env::vars()
        .filter_map(|(name, _)| {
            name.strip_prefix("CARGO_FEATURE_")
                .map(|feature| feature.to_lowercase().replace('_', "-"))
        })
        .collect();
    features.sort();
    let provenance = serde_json::json!({"schema_version":1,"application_hash":source_hash(&root),"lock_hash":digest(lock.replace("\r\n", "\n")),"path_dependencies":path_dependencies,"features":features,"target":env::var("TARGET").expect("target")});
    fs::write(
        output.join("provenance.json"),
        serde_json::to_vec(&provenance).expect("provenance JSON"),
    )
    .expect("write provenance");
    fs::write(
        output.join("versions.json"),
        format!("{{{}}}", entries.join(",")),
    )
    .expect("write build metadata");
}
