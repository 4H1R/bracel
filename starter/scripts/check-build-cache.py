"""Check Cargo's emitted watches or verify that a second build is entirely cached."""

import argparse
import json
from pathlib import Path
import subprocess
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--project", type=Path, default=Path(__file__).resolve().parent.parent)
    parser.add_argument("--watch-output", type=Path)
    parser.add_argument("--release", action="store_true")
    args = parser.parse_args()
    project = args.project.resolve()
    if args.watch_output:
        missing = []
        for line in args.watch_output.read_text().splitlines():
            if line.startswith("cargo:rerun-if-changed="):
                path = Path(line.split("=", 1)[1])
                if not (project / path).exists():
                    missing.append(str(path))
        assert not missing, f"Cargo watches missing paths, causing repeated builds: {missing}"
        print("All watched paths exist.")
        return

    command = ["cargo", "build", "--locked", "--bin", "bracel-starter", "--message-format=json"]
    if args.release:
        command.append("--release")
    results = []
    for name in ("prepare", "unchanged"):
        started = time.perf_counter()
        result = subprocess.run(command, cwd=project, text=True, capture_output=True)
        if result.returncode:
            raise RuntimeError(result.stderr)
        artifacts = [json.loads(line) for line in result.stdout.splitlines() if line.startswith("{")]
        rebuilt = [item["target"]["name"] for item in artifacts
                   if item.get("reason") == "compiler-artifact" and not item["fresh"]]
        results.append({"run": name, "seconds": round(time.perf_counter() - started, 3), "rebuilt": rebuilt})
        print(json.dumps(results[-1]), flush=True)
    assert not results[-1]["rebuilt"], "Unchanged build recompiled targets"


if __name__ == "__main__":
    main()
