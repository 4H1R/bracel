"""Black-box CLI lifecycle acceptance, with local Git releases and real Cargo.

Failure cases are specified before implementation: invalid/existing destinations,
unavailable tools/releases, dirty trees, unsupported dependencies, failed builds,
unchanged preview files, shell-like environment values, and failed migrations.
Docker orchestration is recorded by an executable double; real containers have a
separate smoke script. No public repositories or releases are changed.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import tomllib


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin", required=True)
    parser.add_argument("--artifact", default="target/lifecycle-e2e.json")
    args = parser.parse_args()
    binary = str(Path(args.bin).resolve())
    artifact = Path(args.artifact).resolve()
    evidence = {"ok": False, "binary_sha256": hashlib.sha256(Path(binary).read_bytes()).hexdigest(),
                "scenarios": [], "commands": []}
    env = os.environ.copy()

    def run(root, *cmd, ok=True):
        result = subprocess.run(cmd, cwd=root, env=env, capture_output=True, text=True, timeout=180)
        evidence["commands"].append({"command": list(cmd), "exit_code": result.returncode,
                                     "expected_success": ok})
        assert (result.returncode == 0) == ok, (cmd, result.stdout, result.stderr)
        return result

    def write(root, name, content):
        path = root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content, encoding="utf-8", newline="\n")

    def commit(root, message):
        run(root, "git", "add", ".")
        run(root, "git", "-c", "user.name=Fixture", "-c", "user.email=fixture@example.test", "commit", "-qm", message)
        return run(root, "git", "rev-parse", "HEAD").stdout.strip()

    def passed(name):
        evidence["scenarios"].append(name)

    try:
        with tempfile.TemporaryDirectory(prefix="bracel-lifecycle-") as temporary:
            root = Path(temporary)
            framework = root / "framework"
            starter = root / "starter"
            framework.mkdir()
            starter.mkdir()
            version = run(root, binary, "--version").stdout.strip().split()[-1]
            major, minor, patch = map(int, version.split("."))
            next_version = f"{major}.{minor}.{patch + 1}"
            broken_version = f"{major}.{minor}.{patch + 2}"
            run(framework, "git", "init", "-q")
            write(framework, "Cargo.toml", '[workspace]\nmembers=["crates/bracel", "crates/bracel-integrations"]\nresolver="3"\n')
            for name in ("bracel", "bracel-integrations"):
                write(framework, f"crates/{name}/Cargo.toml", f'[package]\nname="{name}"\nversion="{version}"\nedition="2024"\n[features]\ntesting=[]\n')
                write(framework, f"crates/{name}/src/lib.rs", 'pub fn value() -> u32 { 42 }\n')
            toolchain = (Path(__file__).resolve().parent.parent / "rust-toolchain.toml").read_text()
            write(framework, "rust-toolchain.toml", toolchain)
            write(framework, "CHANGELOG.md", f'# Changes\n\n## {next_version}\nReview application migrations.\n')
            old = commit(framework, "initial framework")
            run(framework, "git", "tag", "v" + version)
            for name in ("bracel", "bracel-integrations"):
                path = framework / f"crates/{name}/Cargo.toml"
                write(framework, str(path.relative_to(framework)), path.read_text().replace(version, next_version))
            new = commit(framework, "compatible release")
            run(framework, "git", "-c", "user.name=Fixture", "-c", "user.email=fixture@example.test", "tag", "-a", "v" + next_version, "-m", "release")
            for name in ("bracel", "bracel-integrations"):
                path = framework / f"crates/{name}/Cargo.toml"
                write(framework, str(path.relative_to(framework)), path.read_text().replace(next_version, broken_version))
            write(framework, "crates/bracel/src/lib.rs", 'compile_error!("fixture incompatible release");\n')
            commit(framework, "incompatible release")
            run(framework, "git", "tag", "v" + broken_version)

            # Redirect the actual production Git URLs to isolated local repositories.
            config = root / "gitconfig"
            write(root, "gitconfig", f'[url "{framework.as_uri()}"]\n insteadOf = https://github.com/4H1R/bracel\n'
                  f'[url "{starter.as_uri()}"]\n insteadOf = https://github.com/4H1R/bracel-starter\n'
                  '[protocol "file"]\n allow = always\n')
            env.update(GIT_CONFIG_GLOBAL=str(config), GIT_CONFIG_NOSYSTEM="1", CARGO_NET_GIT_FETCH_WITH_CLI="true")
            dependency = f'git="https://github.com/4H1R/bracel", rev="{old}", version="{version}"'
            write(starter, "Cargo.toml", f'[package]\nname="bracel-starter"\nversion="{version}"\nedition="2024"\ndefault-run="bracel-starter"\n'
                  f'[dependencies]\nbracel={{ {dependency} }}\nintegrations={{ package="bracel-integrations", {dependency}, optional=true }}\n'
                  f'[dev-dependencies]\nbracel={{ {dependency}, features=["testing"] }}\n[workspace]\n')
            write(starter, "src/lib.rs", 'pub fn value() -> u32 { bracel::value() }\n')
            write(starter, "src/main.rs", 'fn main() { assert_eq!(bracel_starter::value(), 42); println!("{}:{}", std::env::args().nth(1).unwrap_or_default(), std::env::var("SAMPLE").unwrap_or_default()); if std::env::args().nth(1).as_deref() == Some("migrate") && std::env::var("FAIL_MIGRATE").is_ok() { std::process::exit(9); } }\n')
            write(starter, ".env.example", "DATABASE_URL=postgres://starter:starter@127.0.0.1:5432/starter\nSAMPLE='hello world'\n")
            write(starter, ".gitignore", "target/\n.env\n")
            write(starter, "STARTER_VERSION", "Original starter identity\n")
            write(starter, "Dockerfile", "RUN cargo build --bin bracel-starter\n")
            write(starter, "scripts/dev.sh", "cargo run --bin bracel-starter\n")
            write(starter, "rust-toolchain.toml", toolchain)
            run(starter, "cargo", "generate-lockfile")
            run(starter, "git", "init", "-q")
            commit(starter, "starter")
            run(starter, "git", "tag", "v" + version)

            # Portable executable double: no shell quoting or .cmd lookup assumptions.
            fake = root / "tools"
            fake.mkdir()
            write(fake, "docker.rs", r'''use std::{env, fs::OpenOptions, io::Write};
fn main() { let args: Vec<_> = env::args().skip(1).collect();
if let Ok(log) = env::var("DOCKER_LOG") { writeln!(OpenOptions::new().create(true).append(true).open(log).unwrap(), "{}", args.join("|" )).unwrap(); }
if args.iter().any(|a| a == "info") && env::var("FAIL_DOCKER").is_ok() { std::process::exit(1); }
println!("Docker fixture"); }''')
            docker = fake / ("docker.exe" if os.name == "nt" else "docker")
            run(root, "rustc", str(fake / "docker.rs"), "-o", str(docker))
            env["PATH"] = str(fake) + os.pathsep + env["PATH"]
            env["DOCKER_LOG"] = str(root / "docker.log")
            run(root, binary, "setup")
            env["FAIL_DOCKER"] = "1"
            failed = run(root, binary, "setup", ok=False)
            assert "Docker" in failed.stdout + failed.stderr
            del env["FAIL_DOCKER"]
            passed("setup reports unavailable Docker with an actionable message")

            app = root / "my-api"
            run(root, binary, "new", str(app))
            data = tomllib.loads((app / "Cargo.toml").read_text())
            assert data["package"]["name"] == data["package"]["default-run"] == "my-api"
            assert "my_api::" in (app / "src/main.rs").read_text()
            assert "--bin my-api" in (app / "scripts/dev.sh").read_text()
            assert (app / ".env").exists()
            assert (app / "STARTER_VERSION").read_text() == "Original starter identity\n"
            run(app, "cargo", "check", "--locked", "--all-targets")
            run(root, binary, "new", str(app), ok=False)
            run(root, binary, "new", str(root / "bad name"), ok=False)
            assert not (root / "bad name").exists()
            passed("new names package, imports, scripts and lockfile without overwriting a destination")

            env["TEST_DATABASE_URL"] = "postgres://test:test@disposable-tests:5432/test"
            run(app, binary, "dev")
            compose = json.loads((app / "target/bracel/compose.json").read_text())
            assert compose["services"]["tools"]["environment"]["TEST_DATABASE_URL"] == env["TEST_DATABASE_URL"]
            del env["TEST_DATABASE_URL"]
            log = (root / "docker.log").read_text()
            assert "migrate" in log and "serve" in log
            run(app, binary, "down")
            assert "--volumes" not in (root / "docker.log").read_text()
            run(app, binary, "ai", "info")
            assert "/bracel-tools/bin/bracel|ai|info" in (root / "docker.log").read_text()
            original_env = (app / ".env").read_text()
            write(app, ".env", "a temporarily invalid edit\n")
            run(app, binary, "down")
            write(app, ".env", original_env)
            passed("container dev builds, migrates and serves; down retains volumes")

            # Native mode exercises real child processes and literal environment parsing.
            run(app, binary, "dev", "--native", "--no-services")
            env["SAMPLE"] = "process wins"
            result = run(app, binary, "run", "--native", "doctor")
            assert "doctor:process wins" in result.stdout
            del env["SAMPLE"]
            write(app, ".env", "DATABASE_URL=postgres://x:x@127.0.0.1/db\nSAMPLE='$(echo should-not-execute)'\n")
            result = run(app, binary, "run", "--native", "doctor")
            assert "$(echo should-not-execute)" in result.stdout
            env["FAIL_MIGRATE"] = "1"
            result = run(app, binary, "dev", "--native", "--no-services", ok=False)
            assert "serve:" not in result.stdout
            del env["FAIL_MIGRATE"]
            write(app, ".env", "DATABASE_URL=postgres://x:x@production.example/db\n")
            run(app, binary, "dev", "--native", "--no-services", ok=False)
            write(app, ".env", "DATABASE_URL=postgres://x:x@127.0.0.1/db?host=production.example\n")
            run(app, binary, "dev", "--native", "--no-services", ok=False)
            passed("native dev respects process env, treats shell syntax literally and stops on failed or remote migrations")

            pinned_channel = tomllib.loads(toolchain)["toolchain"]["channel"]
            older_toolchain = toolchain.replace(f'"{pinned_channel}"', '"1.0.0"')
            write(app, "rust-toolchain.toml", older_toolchain)
            commit(app, "adopt application")
            before = {p: (app / p).read_bytes() for p in ("Cargo.toml", "Cargo.lock", "rust-toolchain.toml", "src/main.rs", "STARTER_VERSION")}
            run(app, binary, "upgrade", "--check", "--to", next_version)
            assert before == {p: (app / p).read_bytes() for p in before}
            write(app, "custom.txt", "uncommitted work")
            run(app, binary, "upgrade", "--to", next_version, "--native", ok=False)
            (app / "custom.txt").unlink()
            run(app, binary, "upgrade", "--to", next_version, "--native")
            updated = tomllib.loads((app / "Cargo.toml").read_text())
            for section in ("dependencies", "dev-dependencies"):
                for dep in updated[section].values():
                    assert dep["rev"] == new and dep["version"] == next_version
            assert updated["dependencies"]["integrations"]["optional"]
            assert updated["dev-dependencies"]["bracel"]["features"] == ["testing"]
            assert tomllib.loads((app / "rust-toolchain.toml").read_text())["toolchain"]["channel"] == pinned_channel
            assert (app / "src/main.rs").read_bytes() == before["src/main.rs"]
            assert (app / "STARTER_VERSION").read_bytes() == before["STARTER_VERSION"]
            run(app, "cargo", "check", "--locked", "--all-targets")
            passed("upgrade preview is read-only; apply updates aliased and dev dependencies and preserves application files")

            write(app, "rust-toolchain.toml", older_toolchain)
            commit(app, "upgrade")
            before = {p: (app / p).read_bytes() for p in ("Cargo.toml", "Cargo.lock", "rust-toolchain.toml")}
            run(app, binary, "upgrade", "--to", broken_version, "--native", ok=False)
            assert before == {p: (app / p).read_bytes() for p in before}
            run(app, binary, "upgrade", "--to", "999.0.0", "--native", ok=False)
            assert before == {p: (app / p).read_bytes() for p in before}
            passed("failed resolution or compilation restores exact manifest and lockfile bytes")

            # Exercise replacing the running CLI itself, including Windows rename.
            import platform
            target = "x86_64-pc-windows-msvc.exe" if os.name == "nt" else (
                ("aarch64" if platform.machine() == "arm64" else "x86_64") + "-apple-darwin"
                if platform.system() == "Darwin" else "x86_64-unknown-linux-gnu")
            replacement = root / ("replacement.exe" if os.name == "nt" else "replacement")
            write(root, "replacement.rs", f'fn main() {{ println!("bracel {next_version}"); }}\n')
            run(root, "rustc", str(root / "replacement.rs"), "-o", str(replacement))
            write(fake, "curl.rs", r'''use std::{env,fs};
fn main() { let args: Vec<_> = env::args().skip(1).collect();
let dest = &args[args.iter().position(|a| a == "--output").unwrap()+1];
let source = if args.last().unwrap().ends_with("/SHA256SUMS") { "UPDATE_SUMS" } else if args.last().unwrap().ends_with("/latest") { "UPDATE_RELEASE" } else { "UPDATE_BINARY" };
fs::copy(env::var(source).unwrap(), dest).unwrap(); }''')
            run(root, "rustc", str(fake / "curl.rs"), "-o", str(fake / ("curl.exe" if os.name == "nt" else "curl")))
            sums = root / "update-sums"
            env["UPDATE_SUMS"] = str(sums)
            env["UPDATE_BINARY"] = str(replacement)
            write(root, "latest-release.json", json.dumps({"tag_name": "v" + next_version}))
            env["UPDATE_RELEASE"] = str(root / "latest-release.json")
            installed = fake / ("installed.exe" if os.name == "nt" else "installed")
            shutil.copy2(binary, installed)
            sums.write_text(f"{'0' * 64}  bracel-{target}\n")
            run(root, str(installed), "self", "update", "--to", next_version, ok=False)
            assert installed.read_bytes() == Path(binary).read_bytes()
            sums.write_text(f"{hashlib.sha256(replacement.read_bytes()).hexdigest()}  bracel-{target}\n")
            run(root, str(installed), "self", "update")
            assert run(root, str(installed), "--version").stdout.strip() == f"bracel {next_version}"
            assert installed.with_suffix(".previous").read_bytes() == Path(binary).read_bytes()
            passed("self update verifies checksums, replaces the running executable and keeps the previous binary")
            evidence["ok"] = True
    finally:
        artifact.parent.mkdir(parents=True, exist_ok=True)
        artifact.write_text(json.dumps(evidence, indent=2) + "\n", encoding="utf-8")
        print(json.dumps({"ok": evidence["ok"], "scenarios": len(evidence["scenarios"]), "artifact": str(artifact)}))


if __name__ == "__main__":
    main()
