"""Exercise the real CLI and the entire generated application's tests."""
from pathlib import Path
import json
import os
import shutil
import subprocess
import tempfile
import uuid
import hashlib

root = Path(__file__).resolve().parent.parent
cli = Path(os.environ["CARGO_TARGET_DIR"]).resolve() / "debug/bracel"
artifact=root/".scratch"/"generated-e2e"/uuid.uuid4().hex
artifact.mkdir(parents=True)
evidence={"ok":False,"commands":[],"generated":{}}
def run(argv,cwd):
    result=subprocess.run(argv,cwd=cwd,capture_output=True,text=True)
    evidence["commands"].append({"argv":argv,"exit_code":result.returncode})
    (artifact/f"command-{len(evidence['commands'])}.log").write_text(result.stdout+result.stderr)
    (artifact/"report.json").write_text(json.dumps(evidence,indent=2))
    result.check_returncode()
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
    for kind,name in [("job","Archive"),("event","Archived"),("policy","ArchivePolicy"),("command","Reconcile")]:
        run([str(cli),"make",kind,name],app)
    run([str(cli),"make","resource","Asset","--field","name:string","--field","external_id:uuid","--field","due:date?","--field","amount:decimal","--field","state:enum(draft|ready)"],app)
    before = (app / "src/features/projects/mod.rs").read_bytes()
    assert subprocess.run(args, cwd=app, capture_output=True).returncode == 2
    assert subprocess.run([str(cli), "make", "resource", "../escape", "--field", "name:string"], cwd=app, capture_output=True).returncode == 2
    assert (app / "src/features/projects/mod.rs").read_bytes() == before
    run(["cargo", "fmt"], app)
    run(["cargo", "clippy", "--all-targets", "--", "-D", "warnings"], app)
    run(["cargo", "test", "--all-targets"], app)
    run(["bash", "scripts/openapi.sh", "write"], app)
    api = json.loads((app / "docs/openapi.json").read_text())
    assert set(api["paths"]["/projects"]) >= {"get", "post"}
    assert set(api["paths"]["/projects/{id}"]) >= {"get", "put", "delete"}
    assert "application/problem+json" in api["paths"]["/projects"]["post"]["responses"]["422"]["content"]
    operation_ids = [operation["operationId"] for path in api["paths"].values() for method, operation in path.items()
                     if method in {"get", "post", "put", "patch", "delete", "head", "options", "trace"}]
    assert len(operation_ids) == len(set(operation_ids))
    for path in app.glob("src/**/*.rs"):
        evidence["generated"][str(path.relative_to(app))]=hashlib.sha256(path.read_bytes()).hexdigest()
    evidence["ok"]=True
    (artifact/"report.json").write_text(json.dumps(evidence,indent=2))
    (artifact/"openapi.json").write_text(json.dumps(api,indent=2))
    print(f"Generated application evidence: {artifact}")
    print("Generated application: dry run, conflicts, CRUD, ownership, migrations, contracts and all application tests passed.")
