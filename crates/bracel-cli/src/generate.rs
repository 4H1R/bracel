use serde::Serialize;
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

#[derive(Serialize)]
pub struct Change {
    pub path: String,
    pub before: Option<String>,
    pub after: String,
}
#[derive(Serialize)]
pub struct Plan {
    pub schema_version: u32,
    pub changes: Vec<Change>,
}
fn ident(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 40
        && name.as_bytes()[0].is_ascii_lowercase()
        && name
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_')
        && ![
            "self", "super", "crate", "type", "mod", "pub", "use", "fn", "struct", "enum", "impl",
            "trait", "async", "await", "move", "ref", "match", "loop", "if", "else", "const",
            "static", "let", "mut", "dyn", "where", "return", "in", "for", "while", "as", "true",
            "false", "unsafe", "extern", "break", "continue", "yield", "try", "gen", "abstract",
            "become", "box", "do", "final", "macro", "override", "priv", "typeof", "unsized",
            "virtual",
        ]
        .contains(&name)
}
fn pascal(name: &str) -> String {
    name.split('_')
        .map(|part| {
            let mut chars = part.chars();
            chars
                .next()
                .map(|c| c.to_ascii_uppercase().to_string() + chars.as_str())
                .unwrap_or_default()
        })
        .collect()
}
pub fn resource(root: &Path, name: &str, fields: &[String]) -> Result<Plan, String> {
    let manifest = fs::read_to_string(root.join("Cargo.toml"))
        .map_err(|_| "Run inside a Bracel application")?;
    let manifest: toml::Value =
        toml::from_str(&manifest).map_err(|_| "Invalid application manifest")?;
    let crate_name = manifest
        .get("lib")
        .and_then(|lib| lib.get("name"))
        .and_then(toml::Value::as_str)
        .or_else(|| {
            manifest
                .get("package")
                .and_then(|p| p.get("name"))
                .and_then(toml::Value::as_str)
        })
        .ok_or("Application package name is required")?
        .replace('-', "_");
    if !ident(&crate_name) {
        return Err("Application crate name must be a supported Rust identifier".into());
    }
    let module = name.to_ascii_lowercase();
    if !ident(&module) || fields.is_empty() || fields.len() > 32 {
        return Err(
            "Use a resource identifier and 1..32 --field name:string|i64|bool declarations.".into(),
        );
    }
    let table = format!("{module}s");
    let ty = pascal(&module);
    let mut seen = BTreeSet::new();
    let mut definitions = String::new();
    let mut from = Vec::new();
    let mut names = Vec::new();
    let mut validate = String::new();
    let mut fixture = Vec::new();
    let mut set = Vec::new();
    let mut update = String::new();
    let mut filters = String::new();
    let mut columns = Vec::new();
    for field in fields {
        let (name, kind) = field.split_once(':').ok_or("Fields require name:type")?;
        if !ident(name) || ["id", "owner", "created_at"].contains(&name) || !seen.insert(name) {
            return Err("Invalid, reserved or repeated field name".into());
        }
        let (rust, sql, read, example) = match kind {
            "string" => (
                "String",
                "text",
                format!("fields.text(\"{name}\", 200)"),
                "\"example\".into()",
            ),
            "i64" => ("i64", "bigint", format!("fields.integer(\"{name}\")"), "1"),
            "bool" => (
                "bool",
                "boolean",
                format!("fields.boolean(\"{name}\")"),
                "true",
            ),
            _ => return Err("Supported field types: string, i64, bool".into()),
        };
        definitions += &format!("        pub {name}: {rust},\n");
        from.push(format!("{name}: row.{name}"));
        names.push(name);
        set.push(format!("{name}: Set(input.{name})"));
        validate += &format!("        let {name} = {read};\n");
        fixture.push(format!("{name}: {example}"));
        update += &format!(
            "        .col_expr(Column::{}, Expr::value(input.{name}))\n",
            pascal(name)
        );
        columns.push(format!(
            "\\\"{name}\\\" {sql} NOT NULL{}",
            if kind == "string" {
                format!(" CHECK (char_length(\\\"{name}\\\") BETWEEN 1 AND 200)")
            } else {
                String::new()
            }
        ));
        if kind == "string" {
            filters += &format!(
                "            Filter {{ name: \"{name}\", column: Column::{}, kind: FilterKind::TextContains }},\n",
                pascal(name)
            );
        }
    }
    let render = |template: &str| {
        template
            .replace("__CRATE__", &crate_name)
            .replace("__TABLE__", &table)
            .replace("__TYPE__", &ty)
            .replace("__FIELDS__", &definitions)
            .replace("__FROM__", &from.join(", "))
            .replace("__NAMES__", &names.join(", "))
            .replace("__SET__", &set.join(", "))
            .replace("__VALIDATE__", &validate)
            .replace("__FIXTURE__", &fixture.join(", "))
            .replace("__UPDATE__", &update)
            .replace("__FILTERS__", &filters)
            .replace("__COLUMNS__", &columns.join(", "))
    };
    let migration = format!(
        "m{}_create_{table}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| "System clock invalid")?
            .as_secs()
    );
    let mut plan = Plan {
        schema_version: 1,
        changes: vec![],
    };
    plan.new_file(
        root,
        &format!("src/features/{table}/mod.rs"),
        render(include_str!("../templates/resource.rs")),
    )?;
    plan.new_file(
        root,
        &format!("src/migrations/{migration}.rs"),
        render(include_str!("../templates/migration.rs")),
    )?;
    plan.new_file(
        root,
        &format!("tests/{table}.rs"),
        render(include_str!("../templates/test.rs")),
    )?;
    plan.insert(
        root,
        "src/features/mod.rs",
        "// bracel:generated-modules",
        &format!("pub mod {table};\n"),
    )?;
    plan.insert(
        root,
        "src/features/mod.rs",
        "    // bracel:generated-routes",
        &format!("    {table}::register(_registry);\n"),
    )?;
    plan.insert(
        root,
        "src/migrations/mod.rs",
        "// bracel:generated-migrations",
        &format!("mod {migration};\n"),
    )?;
    plan.insert(
        root,
        "src/migrations/mod.rs",
        "            // bracel:generated-migration-list",
        &format!("            Box::new({migration}::Migration),\n"),
    )?;
    Ok(plan)
}
impl Plan {
    fn new_file(&mut self, root: &Path, path: &str, after: String) -> Result<(), String> {
        checked(root, path)?;
        if root.join(path).exists() {
            return Err(format!("File conflict: {path}"));
        }
        self.changes.push(Change {
            path: path.into(),
            before: None,
            after,
        });
        Ok(())
    }
    fn insert(&mut self, root: &Path, path: &str, marker: &str, text: &str) -> Result<(), String> {
        checked(root, path)?;
        if let Some(change) = self.changes.iter_mut().find(|c| c.path == path) {
            if change.after.matches(marker).count() != 1 {
                return Err(format!("Expected one registration marker in {path}"));
            }
            change.after = change.after.replace(marker, &format!("{text}{marker}"));
            return Ok(());
        }
        let before = fs::read_to_string(root.join(path)).map_err(|_| {
            format!("Cannot read {path}; run inside a compatible Bracel application")
        })?;
        if before.matches(marker).count() != 1 {
            return Err(format!("Expected one registration marker in {path}"));
        }
        let after = before.replace(marker, &format!("{text}{marker}"));
        self.changes.push(Change {
            path: path.into(),
            before: Some(before),
            after,
        });
        Ok(())
    }
    pub fn apply(&self, root: &Path) -> Result<(), String> {
        for change in &self.changes {
            checked(root, &change.path)?;
            let current = fs::read_to_string(root.join(&change.path)).ok();
            if current != change.before {
                return Err(format!("File changed or conflicts: {}", change.path));
            }
        }
        for (index, change) in self.changes.iter().enumerate() {
            let path = root.join(&change.path);
            let result = (|| {
                fs::create_dir_all(path.parent().expect("parent"))?;
                if change.before.is_none() {
                    use std::io::Write;
                    let mut file = fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&path)?;
                    file.write_all(change.after.as_bytes())
                } else {
                    fs::write(&path, &change.after)
                }
            })();
            if result.is_err() {
                if let Some(before) = &change.before {
                    let _ = fs::write(&path, before);
                }
                for previous in self.changes[..index].iter().rev() {
                    let path = root.join(&previous.path);
                    if let Some(before) = &previous.before {
                        let _ = fs::write(path, before);
                    } else if fs::read_to_string(&path).is_ok_and(|s| s == previous.after) {
                        let _ = fs::remove_file(path);
                    }
                }
                return Err(
                    "Generation failed; review filesystem permissions and any partial files."
                        .into(),
                );
            }
        }
        Ok(())
    }
}
fn checked(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let root = root
        .canonicalize()
        .map_err(|_| "Application directory unavailable")?;
    let mut path = root;
    for part in Path::new(relative).components() {
        if !matches!(part, std::path::Component::Normal(_)) {
            return Err("Invalid output path".into());
        }
        path.push(part);
        if fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err("Generation through symlinks is not supported".into());
        }
    }
    Ok(path)
}
