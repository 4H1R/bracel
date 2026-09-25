"""Exercise the real CLI and the entire generated application's tests."""
from pathlib import Path
import json
import os
import shutil
import subprocess
import tempfile

root = Path(__file__).resolve().parent.parent
cli = Path(os.environ["CARGO_TARGET_DIR"]).resolve() / "debug/bracel"
(root / ".scratch").mkdir(exist_ok=True)
with tempfile.TemporaryDirectory(prefix="generated-", dir=root / ".scratch") as temporary:
    app = Path(temporary) / "app"
    shutil.copytree(root / "starter", app, ignore=shutil.ignore_patterns(".env", ".scratch", "target", "*.log"))
    manifest = app / "Cargo.toml"
    manifest.write_text(manifest.read_text().replace("../crates/", str(root / "crates") + "/").replace('"bracel-starter"', '"renamed-bracel-app"') + "\n[workspace]\n")
    for file in [*app.glob("src/**/*.rs"), *app.glob("tests/**/*.rs")]:
        file.write_text(file.read_text().replace("bracel_starter", "renamed_bracel_app").replace("CARGO_BIN_EXE_bracel-starter", "CARGO_BIN_EXE_renamed-bracel-app"))
    shutil.copyfile(root / "Cargo.lock", app / "Cargo.lock")
    args = [str(cli), "make", "resource", "Project", "--field", "name:string", "--field", "count:i64", "--field", "active:bool", "--crud"]
    plan = json.loads(subprocess.check_output(args + ["--dry-run", "--json"], cwd=app))
    assert plan["schema_version"] == 1 and len(plan["changes"]) == 5
    assert not (app / "src/features/projects").exists()
    subprocess.run(args, cwd=app, check=True)
    before = (app / "src/features/projects/mod.rs").read_bytes()
    assert subprocess.run(args, cwd=app, capture_output=True).returncode == 2
    assert subprocess.run([str(cli), "make", "resource", "../escape", "--field", "name:string"], cwd=app, capture_output=True).returncode == 2
    assert (app / "src/features/projects/mod.rs").read_bytes() == before
    subprocess.run(["cargo", "fmt"], cwd=app, check=True)
    subprocess.run(["cargo", "clippy", "--all-targets", "--", "-D", "warnings"], cwd=app, check=True)
    subprocess.run(["cargo", "test", "--all-targets"], cwd=app, check=True)
    subprocess.run(["bash", "scripts/openapi.sh", "write"], cwd=app, check=True)
    api = json.loads((app / "docs/openapi.json").read_text())
    assert set(api["paths"]["/projects"]) >= {"get", "post"}
    assert set(api["paths"]["/projects/{id}"]) >= {"get", "put", "delete"}
    assert "application/problem+json" in api["paths"]["/projects"]["post"]["responses"]["422"]["content"]
    operation_ids = [operation["operationId"] for path in api["paths"].values() for method, operation in path.items()
                     if method in {"get", "post", "put", "patch", "delete", "head", "options", "trace"}]
    assert len(operation_ids) == len(set(operation_ids))
    print("Generated application: dry run, conflicts, CRUD, ownership, migrations, contracts and all application tests passed.")
