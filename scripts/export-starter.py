"""Export a standalone starter pinned to a reachable framework Git revision."""
import pathlib
import re
import shutil
import sys

root = pathlib.Path(__file__).resolve().parent.parent
if len(sys.argv) != 3 or not re.fullmatch(r"[0-9a-f]{40}", sys.argv[2]):
    raise SystemExit("Usage: export-starter.py NEW_DIRECTORY FULL_FRAMEWORK_COMMIT")
destination = pathlib.Path(sys.argv[1]).resolve()
if destination.exists():
    raise SystemExit("Destination must not exist")
shutil.copytree(root / "starter", destination,
                ignore=shutil.ignore_patterns(".git", ".env", ".scratch", "target", "*.log"))
manifest = destination / "Cargo.toml"
text = manifest.read_text()
old = 'bracel = { version = "0.1.0", path = "../crates/bracel" }'
new = ('bracel = { version = "0.1.0", git = "https://github.com/4H1R/bracel", '
       f'rev = "{sys.argv[2]}" }}')
if old not in text:
    raise SystemExit("Reference manifest changed; update exporter")
manifest.write_text(text.replace(old, new) + '\n[workspace]\n\n[profile.release]\nstrip = true\nlto = "thin"\n')
shutil.copyfile(root / "Cargo.lock", destination / "Cargo.lock")
(destination / "STARTER_VERSION").write_text(
    f"Bracel starter 0.1.0\nFramework: https://github.com/4H1R/bracel\nRevision: {sys.argv[2]}\n")
print(f"Exported starter to {destination}; update its lockfile and run its checks.")
