"""Exercise the installed AI companion against independent local consumers."""
import argparse
import json
import os
import queue
import shutil
from pathlib import Path
import subprocess
import tempfile
import threading


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin", required=True)
    parser.add_argument("--artifact", default="target/ai-e2e.json")
    args = parser.parse_args()
    binary = str(Path(args.bin).resolve())
    evidence = {"schema_version": 1, "ok": False, "scenarios": [], "revisions": [], "commands": []}
    artifact = Path(args.artifact).resolve()

    def run(cwd, *command, ok=True):
        result = subprocess.run(command, cwd=cwd, text=True, capture_output=True, timeout=90)
        evidence["commands"].append({"command": list(command), "cwd": str(cwd), "exit_code": result.returncode, "expected_success": ok})
        if ok and result.returncode:
            raise AssertionError(f"{command}: {result.stdout}\n{result.stderr}")
        if not ok:
            assert result.returncode, (command, result.stdout)
        return result

    def ai(root, *command, ok=True):
        result = run(root, binary, "ai", *command, ok=ok)
        return json.loads(result.stdout)

    def write(root, path, text):
        dest = root / path
        dest.parent.mkdir(parents=True, exist_ok=True)
        dest.write_text(text)

    def passed(name):
        evidence["scenarios"].append({"name": name, "ok": True})

    try:
        with tempfile.TemporaryDirectory(prefix="bracel-ai-e2e-") as temporary:
            root = Path(temporary)
            framework = root / "framework"
            framework.mkdir()
            write(framework, "Cargo.toml", '[package]\nname="bracel"\nversion="0.1.0"\nedition="2024"\n[features]\nextra=["dep:bracel-extra"]\n[dependencies]\nbracel-extra={path="extra",optional=true}\n')
            write(framework, "extra/Cargo.toml", '[package]\nname="bracel-extra"\nversion="0.1.0"\nedition="2024"\n')
            write(framework, "extra/src/lib.rs", 'pub fn optional_api() {}\n')
            write(framework, "src/lib.rs", "pub fn original_api() {}\n")
            write(framework, "docs/http.md", "# HTTP\nUse original_api for amberrequests.\n")
            write(framework, "ai/guidelines/core.md", "Use the resolved framework revision.\n")
            skill = '---\nname: bracel-http\ndescription: Implement Bracel HTTP routes.\n---\nUse original_api.\n'
            write(framework, "ai/skills/bracel-http/SKILL.md", skill)
            catalog = {"schema_version": 1, "capabilities": [{"id": "http", "crate": "bracel", "status": "implemented", "api": "original_api", "source": "src/lib.rs", "docs": "docs/http.md", "example": "src/lib.rs", "verification": "src/lib.rs"}]}
            catalog["capabilities"].append(dict(catalog["capabilities"][0], id="optional", **{"crate":"bracel-extra", "status":"optional"}))
            catalog["capabilities"].append(dict(catalog["capabilities"][0], id="planned", status="planned"))
            write(framework, "ai/catalog.json", json.dumps(catalog))
            run(framework, "git", "init", "-q")
            run(framework, "git", "add", ".")
            run(framework, "git", "-c", "user.name=Fixture", "-c", "user.email=fixture@example.test", "commit", "-qm", "old")
            old = run(framework, "git", "rev-parse", "HEAD").stdout.strip()
            write(framework, "src/lib.rs", "pub fn original_api() {}\npub fn new_api() {}\n")
            write(framework, "docs/http.md", "# HTTP\nUse new_api for violetrequests.\n")
            run(framework, "git", "add", ".")
            run(framework, "git", "-c", "user.name=Fixture", "-c", "user.email=fixture@example.test", "commit", "-qm", "new")
            new = run(framework, "git", "rev-parse", "HEAD").stdout.strip()
            evidence["revisions"] = [old, new]
            apps = []
            for label, rev in [("old", old), ("new", new), ("local", None)]:
                app = root / label
                app.mkdir()
                dependency = f'git={json.dumps(framework.as_uri())}, rev="{rev}"' if rev else f'path={json.dumps(str(framework))}'
                write(app, "Cargo.toml", '[package]\nname="consumer"\nversion="0.1.0"\nedition="2024"\n[dependencies]\nframework_alias={package="bracel", ' + dependency + '}\n')
                write(app, "src/main.rs", "fn main() {}\n")
                run(app, "cargo", "generate-lockfile")
                write(app, "AGENTS.md", "Handwritten conventions.\n")
                write(app, ".codex/config.toml", '[mcp_servers.existing]\ncommand="existing-server"\n')
                ai(app, "install", "--agents", "codex,claude,cursor")
                assert "Handwritten conventions." in (app / "AGENTS.md").read_text()
                assert "existing-server" in (app / ".codex/config.toml").read_text()
                assert (app / "CLAUDE.md").exists() and (app / ".cursor/rules/bracel.mdc").exists()
                assert ai(app, "sync", "--check")["ok"]
                before = (app / ".bracel/ai.lock.json").read_bytes()
                ai(app, "sync")
                assert before == (app / ".bracel/ai.lock.json").read_bytes()
                apps.append(app)
            assert ai(apps[0], "search", "amberrequests")["results"]
            assert not ai(apps[0], "search", "violetrequests")["results"]
            assert ai(apps[1], "search", "violetrequests")["results"]
            passed("exact Git revisions, renamed dependency, three agent adapters and deterministic sync")
            local = apps[2]
            availability = {c["definition"]["id"]: c["available_in_resolution"] for c in ai(local,"capabilities")["capabilities"]}
            assert availability == {"http":True,"optional":False,"planned":False}
            ai(local,"install","--features","framework_alias/extra")
            assert any(p["name"]=="bracel-extra" for p in ai(local,"info")["project"]["identity"]["packages"])
            configuration=json.loads((local/".bracel/ai.json").read_text())
            configuration["features"]=[]
            (local/".bracel/ai.json").write_text(json.dumps(configuration))
            ai(local,"sync", "--check",ok=False)
            ai(local,"sync")
            manifest_path=apps[0]/"Cargo.toml"
            manifest_path.write_text(manifest_path.read_text().replace(old,new))
            run(apps[0],"cargo","update","--offline")
            ai(apps[0],"sync","--check",ok=False)
            assert ai(apps[0],"search","violetrequests")["results"]
            ai(apps[0],"sync")
            passed("disabled optional and planned capabilities, selected features, dependency revision changes")
            write(framework, "docs/fresh.md", "# Fresh\nuniquefreshfeature\n")
            assert ai(local, "search", "uniquefreshfeature")["results"]
            assert not ai(local, "sync", "--check", ok=False)["ok"]
            ai(local, "sync")
            (framework / "docs/fresh.md").unlink()
            assert not ai(local, "search", "uniquefreshfeature")["results"]
            ai(local, "sync")
            passed("local untracked edits, removal and stale generation detection")
            generated = local / ".agents/skills/bracel-http/SKILL.md"
            generated.write_text(generated.read_text() + "Owner edit\n")
            assert not ai(local, "sync", ok=False)["ok"]
            assert "Owner edit" in generated.read_text()
            generated.write_text(skill)
            ai(local, "sync")
            write(local, ".ai/guidelines/core.md", "Application override sentinel.\n")
            write(local, ".ai/rules/http.md", '---\npaths:\n  - src/**\n---\nUse integer cents.\n')
            ai(local, "sync")
            assert "Application override sentinel" in (local / "AGENTS.md").read_text()
            assert "http.md" in (local / ".ai/rules/index.md").read_text()
            passed("edited output protection, guideline overrides and durable rule index")
            (framework / "ai/skills/bracel-http/SKILL.md").unlink()
            ai(local, "sync")
            assert not generated.exists()
            write(framework, "ai/skills/bracel-http/SKILL.md", skill)
            ai(local, "sync")
            (local / "AGENTS.md").write_text((local / "AGENTS.md").read_text().replace("Application override sentinel", "Edited generated sentinel"))
            ai(local, "sync", ok=False)
            assert "Edited generated sentinel" in (local / "AGENTS.md").read_text()
            (local / "AGENTS.md").write_text((local / "AGENTS.md").read_text().replace("Edited generated sentinel", "Application override sentinel"))
            passed("stale skill retirement and generated block conflict protection")

            config_path = local / ".bracel/ai.json"
            config = json.loads(config_path.read_text())
            inspector = local / ("inspect-fixture.exe" if os.name == "nt" else "inspect-fixture")
            write(local, "inspection-fixture.rs", '''
fn main() {
    assert_eq!(std::env::args().skip(1).collect::<Vec<_>>(), ["inspect", "--json"]);
    let response = std::fs::read_to_string("inspection-response.txt").unwrap();
    match response.as_str() {
        "timeout" => std::thread::sleep(std::time::Duration::from_secs(5)),
        "oversized" => println!("{}", "x".repeat(2_000_001)),
        _ => println!("{}", response),
    }
}
''')
            run(local,"rustc","inspection-fixture.rs","-o",str(inspector))
            config["inspection_executable"] = inspector.name
            config_path.write_text(json.dumps(config))
            def inspection_program(response):
                (local/"inspection-response.txt").write_text(response if isinstance(response,str) else json.dumps(response))
            inspection_program({'schema_version':1,'command':'inspect','ok':True,'application':{}})
            assert ai(local, "inspect", ok=False)["compiled_state"] == "unknown"
            identity = ai(local, "info")["project"]["identity"]
            provenance = {"schema_version": 1, "application_hash": identity["application_hash"], "lock_hash": identity["lock_hash"], "features": [], "target": "fixture", "path_dependencies": {p["name"]: p["source_hash"] for p in identity["packages"]}}
            inspection_program({"schema_version":1,"command":"inspect","ok":True,"application":{"build_provenance":provenance}})
            assert ai(local, "inspect")["compiled_state"].startswith("source_matches")
            write(local, "src/main.rs", "fn main() { /* changed */ }\n")
            assert ai(local, "inspect", ok=False)["compiled_state"] == "stale"
            inspection_program("not JSON")
            ai(local, "inspect", ok=False)
            inspection_program({'schema_version':999,'command':'inspect','ok':True})
            ai(local, "inspect", ok=False)
            inspection_program("oversized")
            ai(local, "inspect", ok=False)
            inspection_program("timeout")
            config["inspection_timeout_seconds"] = 1
            config_path.write_text(json.dumps(config))
            assert "timed out" in ai(local, "inspect", ok=False)["error"]
            config["inspection_executable"] = None
            config_path.write_text(json.dumps(config))
            passed("fixed inspection arguments, unknown/current/stale binaries, invalid contract, bounded output and timeout")

            bundle = root / "bundle"
            ai(local, "bundle", "--output", str(bundle))
            assert (bundle / "bundle.json").exists()
            assert (bundle / "ai/catalog.json").exists()
            shutil.copytree(bundle, local / "knowledge")
            config["knowledge_bundle"] = "knowledge"
            config_path.write_text(json.dumps(config))
            assert ai(local, "search", "violetrequests")["results"]
            write(local, "knowledge/ai/guidelines/unlisted.md", "Unlisted instruction")
            ai(local, "search", "violetrequests", ok=False)
            (local / "knowledge/ai/guidelines/unlisted.md").unlink()
            (local / "knowledge/docs/http.md").write_text("tampered")
            ai(local, "search", "violetrequests", ok=False)
            config["knowledge_bundle"] = None
            config_path.write_text(json.dumps(config))
            passed("portable knowledge bundle consumed independently and integrity mismatch rejected")

            missing = root / "missing"
            missing.mkdir()
            write(missing, "Cargo.toml", '[package]\nname="missing"\nversion="0.1.0"\nedition="2024"\n[dependencies]\nbracel={git="file:///missing-bracel-fixture",rev="' + "0"*40 + '"}\n')
            write(missing, "src/main.rs", "fn main() {}\n")
            assert "DEPENDENCIES.UNRESOLVED" in ai(missing, "install", ok=False)["error"]
            assert not (missing / "AGENTS.md").exists()
            passed("unavailable offline dependency produces explicit failure without generated files")

            saved_catalog = (framework / "ai/catalog.json").read_text()
            (framework / "ai/catalog.json").unlink()
            assert ai(local, "search", "violetrequests")["warnings"]
            write(framework, "ai/catalog.json", saved_catalog)
            if os.name != "nt":
                skill_dir = local / ".agents/skills/bracel-http"
                saved = local / ".agents/skills/saved"
                skill_dir.rename(saved)
                skill_dir.symlink_to(root, target_is_directory=True)
                ai(local, "sync", ok=False)
                skill_dir.unlink()
                saved.rename(skill_dir)
            escaped = dict(catalog)
            escaped["capabilities"] = [dict(catalog["capabilities"][0], docs="../outside.md")]
            write(framework, "ai/catalog.json", json.dumps(escaped))
            ai(local, "sync", ok=False)
            write(framework, "ai/catalog.json", saved_catalog)
            ai(local, "sync")
            passed("source-only fallback, unsafe catalog references and generated symlink protection")

            proc = subprocess.Popen([binary, "ai", "mcp"], cwd=local, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
            responses=queue.Queue()
            def read_responses():
                for line in proc.stdout:
                    responses.put(line)
                responses.put("")
            threading.Thread(target=read_responses,daemon=True).start()
            def rpc(identity, method, params):
                proc.stdin.write(json.dumps({"jsonrpc": "2.0", "id": identity, "method": method, "params": params}) + "\n")
                proc.stdin.flush()
                while True:
                    line = responses.get(timeout=30)
                    assert line, proc.stderr.read()
                    response = json.loads(line)
                    if response.get("id") == identity:
                        evidence["commands"].append({"protocol":"MCP","method":method,"params":params,"response":response})
                        return response
            try:
                assert "result" in rpc(1, "initialize", {"protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": {"name": "acceptance", "version": "1"}})
                proc.stdin.write('{"jsonrpc":"2.0","method":"notifications/initialized"}\n')
                proc.stdin.flush()
                listed = rpc(2, "tools/list", {})["result"]["tools"]
                assert {t["name"] for t in listed} == {"project_info", "search_docs", "capabilities", "inspect_application", "doctor"}
                for identity, name in enumerate(["project_info", "capabilities", "doctor", "inspect_application"], 3):
                    assert "result" in rpc(identity, "tools/call", {"name": name, "arguments": {}})
                write(framework, "docs/live.md", "# Live\nmcplivesentinel\n")
                result = rpc(10, "tools/call", {"name": "search_docs", "arguments": {"query": "mcplivesentinel"}})["result"]
                assert "mcplivesentinel" in json.dumps(result)
                bad = rpc(11, "tools/call", {"name": "search_docs", "arguments": {"query": ""}})
                assert "error" in bad or bad["result"].get("isError")
                unknown = rpc(12,"tools/call",{"name":"unknown_tool","arguments":{}})
                assert "error" in unknown or unknown["result"].get("isError")
            finally:
                proc.stdin.close()
                try:
                    proc.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    proc.kill()
                    proc.wait()
            passed("MCP handshake, discovery, all five tools and live refresh in one session")
            catalog["schema_version"] = 999
            write(framework, "ai/catalog.json", json.dumps(catalog))
            assert not ai(local, "sync", ok=False)["ok"]
            passed("unsupported knowledge schema fails explicitly")
        evidence["ok"] = True
    finally:
        artifact.parent.mkdir(parents=True, exist_ok=True)
        artifact.write_text(json.dumps(evidence, indent=2) + "\n")
        print(json.dumps(evidence, indent=2))


if __name__ == "__main__":
    main()
