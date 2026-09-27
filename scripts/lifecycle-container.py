"""Run a real new project through Docker build, migrations, HTTP and mail.

Only the disposable project's Compose resources are removed. Evidence and logs
are retained under the chosen artifact directory. Uses the CLI's matching
published starter tag; this is separate from the offline release fixtures.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time
import urllib.error
import urllib.request


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin", required=True)
    parser.add_argument("--artifact", default="target/lifecycle-container.json")
    args = parser.parse_args()
    binary = str(Path(args.bin).resolve())
    artifact = Path(args.artifact).resolve()
    artifact.parent.mkdir(parents=True, exist_ok=True)
    log_path = artifact.with_suffix(".log")
    evidence = {"ok": False, "binary_sha256": hashlib.sha256(Path(binary).read_bytes()).hexdigest(), "checks": []}
    process = None

    def request(url, data=None, token=None):
        headers = {"Content-Type": "application/json"}
        if token:
            headers["Authorization"] = "Bearer " + token
        req = urllib.request.Request(url, data=json.dumps(data).encode() if data else None, headers=headers)
        with urllib.request.urlopen(req, timeout=5) as response:
            return response.status, json.load(response)

    try:
        with tempfile.TemporaryDirectory(prefix="bracel-container-") as temporary, log_path.open("w", encoding="utf-8") as log:
            app = Path(temporary) / "onboarding-api"
            subprocess.run([binary, "new", str(app)], check=True, stdout=log, stderr=log, timeout=120)
            settings = json.loads((app / ".bracel/project.json").read_text())
            base = f'http://127.0.0.1:{settings["api_port"]}'
            mail = f'http://127.0.0.1:{settings["mail_ui_port"]}'
            try:
                process = subprocess.Popen([binary, "dev"], cwd=app, stdout=log, stderr=log)
                deadline = time.monotonic() + 1200
                while True:
                    if process.poll() is not None:
                        raise AssertionError(f"bracel dev exited early; inspect {log_path}")
                    try:
                        status, _ = request(base + "/readyz")
                        if status == 200:
                            break
                    except (OSError, ValueError):
                        pass
                    if time.monotonic() >= deadline:
                        raise AssertionError(f"API readiness timed out; inspect {log_path}")
                    time.sleep(2)
                evidence["checks"].append("bracel new + dev reaches migrated readiness")
                status, account = request(base + "/api/auth/register", {"email": "onboarding@example.test", "display_name": "Onboarding", "password": "a-long-onboarding-test-password"})
                assert status in (200, 201)
                token = account["data"]["access_token"]
                status, _ = request(base + "/api/users/me", token=token)
                assert status == 200
                evidence["checks"].append("registration and authenticated profile")
                request(base + "/api/auth/forgot-password", {"email": "onboarding@example.test"})
                deadline = time.monotonic() + 45
                while True:
                    _, messages = request(mail + "/api/v1/messages")
                    if messages.get("total", 0) > 0:
                        break
                    if time.monotonic() >= deadline:
                        raise AssertionError("Mail worker did not deliver to Mailpit")
                    time.sleep(1)
                evidence["checks"].append("account worker delivers password reset to local Mailpit")
                subprocess.run([binary, "ai", "install", "--agents", "codex"], cwd=app, check=True, stdout=log, stderr=log, timeout=300)
                context = subprocess.run([binary, "ai", "info"], cwd=app, check=True, capture_output=True, text=True, timeout=90)
                assert json.loads(context.stdout)["ok"]
                evidence["checks"].append("AI companion installs and resolves dependencies inside Docker")
                subprocess.run([binary, "down"], cwd=app, check=True, stdout=log, stderr=log, timeout=90)
                process.wait(timeout=30)
                process = None
                # The explicit doctor command reuses the same database volume.
                subprocess.run([binary, "run", "doctor", "--database", "--json"], cwd=app, check=True, stdout=log, stderr=log, timeout=180)
                evidence["checks"].append("database remains migrated after down and restart")
                evidence["ok"] = True
            finally:
                compose_file = app / "target/bracel/compose.json"
                if compose_file.exists():
                    # The generated name is deterministic for this unique test path.
                    canonical = str(app.resolve()).replace("\\", "/")
                    name = "onboarding-api-" + hashlib.sha256(canonical.encode()).hexdigest()[:8]
                    subprocess.run(["docker", "compose", "--project-name", name, "--project-directory", str(app), "--env-file", str(app / "target/bracel/empty.env"), "-f", str(compose_file), "down", "--volumes", "--remove-orphans"], stdout=log, stderr=log, timeout=90, check=True)
                if process is not None:
                    try:
                        process.wait(timeout=15)
                    except subprocess.TimeoutExpired:
                        process.terminate()
                        process.wait(timeout=15)
    finally:
        artifact.write_text(json.dumps(evidence, indent=2) + "\n")
        print(json.dumps({**evidence, "log": str(log_path)}))


if __name__ == "__main__":
    main()
