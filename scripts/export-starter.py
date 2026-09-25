"""Export the reference application against an exact framework revision."""
from pathlib import Path
import re
import shutil
import sys
import tomllib

ROOT = Path(__file__).resolve().parent.parent
REPOSITORY = "https://github.com/4H1R/bracel"


def export_starter(destination: Path, revision: str, root: Path = ROOT) -> None:
    destination = destination.resolve()
    source = (root / "starter").resolve()
    if not re.fullmatch(r"[0-9a-f]{40}", revision):
        raise ValueError("Framework revision must be a full 40-character commit SHA")
    if destination.is_relative_to(source):
        raise ValueError("Destination must be outside the reference starter")
    if destination.exists():
        raise ValueError("Destination must not exist")

    manifest = (source / "Cargo.toml").read_text()
    starter = tomllib.loads(manifest)
    framework = tomllib.loads((root / "crates/bracel/Cargo.toml").read_text())
    cli = tomllib.loads((root / "crates/bracel-cli/Cargo.toml").read_text())
    version = framework["package"]["version"]
    if any(package["package"]["version"] != version for package in (starter, cli)):
        raise ValueError("Framework, CLI and starter versions must match")
    dependency = starter["dependencies"]["bracel"]
    if dependency != {"version": version, "path": "../crates/bracel"}:
        raise ValueError("Starter must depend on the matching local Bracel version")
    if "workspace" in starter or "profile" in starter:
        raise ValueError("Reference starter must inherit the workspace build profiles")

    replacement = f'bracel = {{ version = "{version}", git = "{REPOSITORY}", rev = "{revision}" }}'
    manifest, count = re.subn(r"^bracel\s*=\s*\{[^\n]*\}$", replacement, manifest, flags=re.M)
    if count != 1:
        raise ValueError("Expected one inline Bracel dependency declaration")
    lock = (root / "Cargo.lock").read_bytes()
    manifest += '\n[workspace]\n\n[profile.release]\nstrip = true\nlto = "thin"\n'

    shutil.copytree(source, destination,
                    ignore=shutil.ignore_patterns(".git", ".env", ".scratch", "target", "*.log"))
    (destination / "Cargo.toml").write_text(manifest)
    (destination / "Cargo.lock").write_bytes(lock)
    (destination / "STARTER_VERSION").write_text(
        f"Bracel starter {version}\nFramework: {REPOSITORY}\nRevision: {revision}\n")


def main() -> None:
    if len(sys.argv) != 3:
        raise SystemExit("Usage: export-starter.py NEW_DIRECTORY FULL_FRAMEWORK_COMMIT")
    try:
        export_starter(Path(sys.argv[1]), sys.argv[2])
    except ValueError as error:
        raise SystemExit(str(error)) from error
    print(f"Exported starter to {sys.argv[1]}; update its lockfile and run its checks.")


if __name__ == "__main__":
    main()
