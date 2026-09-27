"""Export the reference application against an exact framework revision."""
from pathlib import Path
import re
import shutil
import sys
import tomllib
import subprocess
import tempfile
import tarfile
import io
import hashlib
import json
from urllib.parse import quote, unquote, urlsplit, urlunsplit

ROOT = Path(__file__).resolve().parent.parent
REPOSITORY = "https://github.com/4H1R/bracel"


def pin_documentation_links(content: str, source_file: Path, source: Path, revision: str) -> str:
    """Keep app links local and pin links leaving the starter to its framework revision."""
    source = source.resolve()

    def replace(match):
        raw = match.group(2)
        wrapped = raw.startswith("<")
        url = urlsplit(raw[1:-1] if wrapped else raw)
        if url.scheme or url.netloc or not url.path or url.path.startswith("/"):
            return match.group(0)
        target = (source_file.parent / unquote(url.path)).resolve()
        if target.is_relative_to(source):
            return match.group(0)
        if not target.is_relative_to(source.parent):
            raise ValueError(f"Documentation link leaves the framework: {raw}")
        path = quote(target.relative_to(source.parent).as_posix(), safe="/")
        pinned = urlunsplit(("https", "github.com", f"/4H1R/bracel/blob/{revision}/{path}", url.query, url.fragment))
        return match.group(1) + (f"<{pinned}>" if wrapped else pinned)

    lines = []
    fence = None
    for line in content.splitlines(keepends=True):
        marker = re.match(r"^\s{0,3}(`{3,}|~{3,})", line)
        if marker:
            token = marker.group(1)
            if fence is None:
                fence = token
            elif token[0] == fence[0] and len(token) >= len(fence):
                fence = None
            lines.append(line)
            continue
        if fence is None:
            line = re.sub(r"(!?\[[^\]\n]*\]\(\s*)(<[^>]+>|[^\s)]+)", replace, line)
            line = re.sub(r"^(\s{0,3}\[[^\]]+\]:\s*)(<[^>]+>|\S+)", replace, line)
        lines.append(line)
    return "".join(lines)


def _profile_tables(profiles: dict) -> str:
    """Serialize Cargo profile tables without carrying over workspace metadata."""
    lines = []

    def quoted(value):
        return json.dumps(value, ensure_ascii=False).replace("\x7f", "\\u007f")

    def table(path, settings):
        lines.append("\n[" + ".".join(quoted(part) for part in path) + "]")
        for name, value in settings.items():
            if isinstance(value, dict):
                continue
            if not isinstance(value, (str, int, bool)):
                raise ValueError("Unsupported Cargo profile value: " + ".".join((*path, name)))
            lines.append(f"{quoted(name)} = {quoted(value)}")
        for name, value in settings.items():
            if isinstance(value, dict):
                table((*path, name), value)

    for name, settings in profiles.items():
        table(("profile", name), settings)
    return "\n".join(lines) + "\n" if lines else ""


def export_starter(destination: Path, revision: str, root: Path = ROOT) -> None:
    destination = destination.resolve()
    source = (root / "starter").resolve()
    if not re.fullmatch(r"[0-9a-f]{40}", revision):
        raise ValueError("Framework revision must be a full 40-character commit SHA")
    if destination.is_relative_to(source):
        raise ValueError("Destination must be outside the reference starter")
    if destination.exists():
        raise ValueError("Destination must not exist")

    try:
        resolved = subprocess.check_output(
            ["git", "-C", str(root), "rev-parse", "--verify", revision + "^{commit}"],
            stderr=subprocess.DEVNULL, text=True).strip()
        archive = subprocess.check_output(
            ["git", "-C", str(root), "archive", "--format=tar", resolved,
             "starter", "crates", "Cargo.toml", "Cargo.lock"], stderr=subprocess.DEVNULL)
    except subprocess.CalledProcessError as error:
        raise ValueError("Framework revision must identify an available commit") from error
    with tempfile.TemporaryDirectory(prefix="bracel-export-") as temporary:
        snapshot = Path(temporary) / "source"
        snapshot.mkdir()
        with tarfile.open(fileobj=io.BytesIO(archive)) as contents:
            for member in contents:
                target = (snapshot / member.name).resolve()
                if not target.is_relative_to(snapshot) or not (member.isfile() or member.isdir()):
                    raise ValueError("Export source must contain regular files and directories")
                if member.isdir():
                    target.mkdir(parents=True, exist_ok=True)
                else:
                    target.parent.mkdir(parents=True, exist_ok=True)
                    target.write_bytes(contents.extractfile(member).read())
                    target.chmod(member.mode)
        # Publish only a complete export, on the destination filesystem.
        with tempfile.TemporaryDirectory(prefix=".bracel-export-", dir=destination.parent) as stage:
            output = Path(stage) / "application"
            _export_snapshot(output, resolved, snapshot)
            output.rename(destination)


def _export_snapshot(destination: Path, revision: str, root: Path) -> None:
    source = root / "starter"

    manifest = (source / "Cargo.toml").read_text(encoding="utf-8")
    starter = tomllib.loads(manifest)
    workspace = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))
    framework = tomllib.loads((root / "crates/bracel/Cargo.toml").read_text(encoding="utf-8"))
    cli = tomllib.loads((root / "crates/bracel-cli/Cargo.toml").read_text(encoding="utf-8"))
    version = framework["package"]["version"]
    if any(package["package"]["version"] != version for package in (starter, cli)):
        raise ValueError("Framework, CLI and starter versions must match")
    if "workspace" in starter or "profile" in starter:
        raise ValueError("Reference starter must inherit the workspace build profiles")

    declarations = []
    for section in ("dependencies", "dev-dependencies"):
        for name, dependency in starter.get(section, {}).items():
            if name != "bracel" and not name.startswith("bracel-"):
                continue
            if dependency.get("version") != version or dependency.get("path") != f"../crates/{name}":
                raise ValueError("Starter must depend on the matching local Bracel version")
            if set(dependency) - {"version", "path", "features", "optional", "default-features"}:
                raise ValueError("Unsupported framework dependency option")
            if name.startswith("bracel-"):
                integration = tomllib.loads((root / f"crates/{name}/Cargo.toml").read_text(encoding="utf-8"))
                if integration["package"]["version"] != version:
                    raise ValueError("Framework, CLI and starter versions must match")
            declarations.append(name)
    if "bracel" not in declarations:
        raise ValueError("Starter requires Bracel")
    def pin(match):
        line = match.group(0)
        name = match.group(1)
        line, count = re.subn(r'path\s*=\s*"\.\./crates/' + re.escape(name) + r'"',
                              f'git = "{REPOSITORY}", rev = "{revision}"', line)
        if count != 1:
            raise ValueError("Expected a local framework path")
        return line
    manifest, count = re.subn(r"^(bracel(?:-[a-z]+)?)\s*=\s*\{[^\n]*\}$", pin, manifest, flags=re.M)
    if count != len(declarations):
        raise ValueError("Expected inline Bracel dependency declarations")
    lock = (root / "Cargo.lock").read_bytes()
    manifest += '\n[workspace]\n' + _profile_tables(workspace.get("profile", {}))

    shutil.copytree(source, destination,
                    ignore=shutil.ignore_patterns(".git", ".env", ".scratch", "target", "*.log"))
    for document in destination.rglob("*.md"):
        content = document.read_text(encoding="utf-8")
        pinned = pin_documentation_links(content, source / document.relative_to(destination), source, revision)
        if pinned != content:
            document.write_text(pinned, encoding="utf-8", newline="\n")
    (destination / "Cargo.toml").write_text(manifest, encoding="utf-8", newline="\n")
    (destination / "Cargo.lock").write_bytes(lock)
    (destination / "STARTER_VERSION").write_text(
        f"Bracel starter {version}\nFramework: {REPOSITORY}\nRevision: {revision}\n",
        encoding="utf-8", newline="\n")
    hashes = {str(path.relative_to(destination)).replace("\\", "/"): hashlib.sha256(path.read_bytes()).hexdigest()
              for path in sorted(destination.rglob("*")) if path.is_file() and path.name != "EXPORT_MANIFEST.json"}
    (destination / "EXPORT_MANIFEST.json").write_text(json.dumps({
        "schema_version": 1, "source_revision": revision, "framework_revision": revision,
        "sha256": hashes,
    }, indent=2, sort_keys=True) + "\n", encoding="utf-8", newline="\n")


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
