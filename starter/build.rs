use std::{env, fs, path::PathBuf};

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
    fs::write(
        output.join("versions.json"),
        format!("{{{}}}", entries.join(",")),
    )
    .expect("write build metadata");
}
