"""Validate canonical knowledge and optionally enforce feature/doc changes in a diff."""
import argparse
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parent.parent


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--base", help="Compare public feature edits to this Git revision")
    args = parser.parse_args()
    catalog = json.loads((ROOT / "ai/catalog.json").read_text())
    assert catalog["schema_version"] == 1
    identities = set()
    for capability in catalog["capabilities"]:
        assert capability["id"] not in identities, "Duplicate capability"
        identities.add(capability["id"])
        assert capability["status"] in ("implemented", "optional", "recipe", "planned")
        for field in ("source", "docs", "example", "verification"):
            path = (ROOT / capability[field]).resolve()
            assert path.is_relative_to(ROOT) and path.is_file(), (capability["id"], field)
        assert capability["api"] and capability["crate"]
    for skill in (ROOT / "ai/skills").glob("*/SKILL.md"):
        content = skill.read_text()
        assert content.startswith("---\n") and "\nname:" in content and "\ndescription:" in content, skill
    if args.base:
        changed = subprocess.check_output(["git", "diff", "--name-only", args.base, "--"], cwd=ROOT, text=True).splitlines()
        features = [p for p in changed if (p.startswith("crates/") and "/src/" in p and p.endswith(".rs")) or p.startswith("starter/src/features/")]
        knowledge = [p for p in changed if p.startswith(("ai/", "docs/", "starter/docs/"))]
        assert not features or knowledge, "Feature code changed without documentation or canonical AI knowledge updates"
    print(json.dumps({"ok": True, "capabilities": len(identities), "diff_checked": bool(args.base)}))


if __name__ == "__main__":
    main()
