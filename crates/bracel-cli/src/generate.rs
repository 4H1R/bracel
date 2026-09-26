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
    let mut schema_fields = Vec::new();
    for field in fields {
        let (name, kind) = field.split_once(':').ok_or("Fields require name:type")?;
        if !ident(name) || ["id", "owner", "created_at"].contains(&name) || !seen.insert(name) {
            return Err("Invalid, reserved or repeated field name".into());
        }
        let nullable = kind.ends_with('?');
        let kind = kind.strip_suffix('?').unwrap_or(kind);
        let (rust, sql, rule, example) = match kind {
            "string" => (
                "String",
                "text",
                "Rule::Text { min:1,max:200 }".to_owned(),
                "\"example\".into()".to_owned(),
            ),
            "i64" => (
                "i64",
                "bigint",
                "Rule::Integer { min:i64::MIN,max:i64::MAX }".into(),
                "1".into(),
            ),
            "bool" => ("bool", "boolean", "Rule::Boolean".into(), "true".into()),
            "uuid" => ("Uuid", "uuid", "Rule::Uuid".into(), "Uuid::now_v7()".into()),
            "date" => (
                "String",
                "text",
                "Rule::Timestamp".into(),
                "\"2026-01-01T00:00:00Z\".into()".into(),
            ),
            "decimal" => (
                "String",
                "text",
                "Rule::Decimal { precision:18,scale:4 }".into(),
                "\"1.0000\".into()".into(),
            ),
            kind if kind.starts_with("enum(") && kind.ends_with(')') => {
                let values = kind[5..kind.len() - 1].split('|').collect::<Vec<_>>();
                if values.is_empty() || values.len() > 32 || values.iter().any(|v| !ident(v)) {
                    return Err("Enums require 1..32 identifier values separated by |".into());
                }
                (
                    "String",
                    "text",
                    format!(
                        "Rule::Enum {{ values:vec![{}] }}",
                        values
                            .iter()
                            .map(|v| format!("{v:?}.into()"))
                            .collect::<Vec<_>>()
                            .join(",")
                    ),
                    format!("{:?}.into()", values[0]),
                )
            }
            _ => return Err(
                "Types: string, i64, bool, uuid, date, decimal, enum(a|b); append ? for nullable"
                    .into(),
            ),
        };
        let rust = if nullable {
            format!("Option<{rust}>")
        } else {
            rust.into()
        };
        let read = format!("fields.rule::<{rust}>(\"{name}\", {rule}, {nullable})");
        let example = if nullable {
            format!("Some({example})")
        } else {
            example
        };
        schema_fields.push(format!(
            "Field::required(\"{name}\",{rule}){}",
            if nullable {
                ".optional().nullable()"
            } else {
                ""
            }
        ));
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
            "\\\"{name}\\\" {sql} {}{}",
            if nullable { "" } else { "NOT NULL" },
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
            .replace("__SCHEMA_FIELDS__", &schema_fields.join(","))
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

pub fn extension(root: &Path, kind: &str, name: &str) -> Result<Plan, String> {
    let module = name.to_ascii_lowercase();
    if !ident(&module) {
        return Err("Use a Rust identifier for the extension name".into());
    }
    let ty = pascal(&module);
    let mut plan = Plan {
        schema_version: 1,
        changes: vec![],
    };
    if kind == "migration" {
        let module = format!(
            "m{}_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|_| "Invalid clock")?
                .as_secs(),
            module
        );
        plan.new_file(root,&format!("src/migrations/{module}.rs"),"use sea_orm_migration::prelude::*;\n#[derive(DeriveMigrationName)]\npub struct Migration;\n#[async_trait::async_trait]\nimpl MigrationTrait for Migration {\nasync fn up(&self,_manager:&SchemaManager)->Result<(),DbErr>{Err(DbErr::Custom(\"Implement this migration before applying it\".into()))}\nasync fn down(&self,_manager:&SchemaManager)->Result<(),DbErr>{Err(DbErr::Custom(\"Implement the reversal before using it\".into()))}\n}\n".into())?;
        plan.insert(
            root,
            "src/migrations/mod.rs",
            "// bracel:generated-migrations",
            &format!("mod {module};\n"),
        )?;
        plan.insert(
            root,
            "src/migrations/mod.rs",
            "            // bracel:generated-migration-list",
            &format!("            Box::new({module}::Migration),\n"),
        )?;
        return Ok(plan);
    }
    let code=match kind {
        "job"=>format!("use serde::{{Serialize,Deserialize}};\n#[derive(Serialize,Deserialize)]\n#[serde(deny_unknown_fields)]\npub struct {ty} {{ pub id:uuid::Uuid }}\nimpl bracel::jobs::TypedJob for {ty} {{ const KIND:&'static str=\"{module}\"; }}\npub async fn handle(_job:{ty})->Result<(),bracel::jobs::Failure>{{Err(bracel::jobs::Failure::Permanent(\"not_implemented\"))}}\n"),
        "event"=>format!("use serde::Serialize;\n#[derive(Serialize)]\npub struct {ty} {{ pub id:uuid::Uuid }}\nimpl bracel_realtime::DomainEvent for {ty} {{ const KIND:&'static str=\"{module}\"; }}\n"),
        "policy"=>format!("pub fn authorize(principal:&bracel::identity::Principal,owner:&str)->bool {{ principal.allows(\"{module}:write\") && principal.cursor_scope()==owner }}\n"),
        "command"=>"pub async fn run(_db:sea_orm::DatabaseConnection,_args:Vec<String>)->Result<serde_json::Value,&'static str>{Err(\"Implement the command before running it\")}\n".into(),
        _=>return Err("Supported generators: resource, job, event, policy, command, migration".into()),
    };
    plan.new_file(root, &format!("src/extensions/{module}.rs"), code)?;
    plan.insert(
        root,
        "src/extensions.rs",
        "// bracel:extension-modules",
        &format!(
            "{}pub mod {module};\n",
            if kind == "event" {
                "#[cfg(feature = \"batteries\")]\n"
            } else {
                ""
            }
        ),
    )?;
    if kind == "job" {
        plan.insert(
            root,
            "src/extensions.rs",
            "    // bracel:extension-jobs",
            &format!("    _worker.register_typed({module}::handle).expect(\"unique job kind\");\n"),
        )?;
    }
    if kind == "command" {
        plan.insert(root,"src/extensions.rs","    // bracel:extension-commands",&format!("    _commands.register(bracel::commands::CommandInfo{{name:\"{module}\",summary:\"Application command\",arguments:&[]}}, {module}::run).expect(\"unique command\");\n"))?;
    }
    Ok(plan)
}
