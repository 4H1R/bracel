"""Exercise benchmark process configuration without compiling Rust."""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("benchmark", Path(__file__).with_name("benchmark-build.py"))
benchmark = importlib.util.module_from_spec(spec)
spec.loader.exec_module(benchmark)


class BenchmarkIsolation(unittest.TestCase):
    def test_external_compiler_caches_are_disabled(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            project = root / "project"
            project.mkdir()
            (project / "src").mkdir()
            for name in ("Cargo.toml", "Cargo.lock", "build.rs", "rust-toolchain.toml", "src/main.rs", "src/config.rs"):
                (project / name).write_text("fixture")
            output = root / "result"
            captured = []

            def run(command, **kwargs):
                captured.append(kwargs["env"])
                kwargs["stdout"].write(json.dumps({"reason": "build-finished", "success": True}) + "\n")
                return subprocess.CompletedProcess(command, 0)

            wrappers = ("RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER", "CARGO_BUILD_RUSTC_WRAPPER", "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER")
            environment = {name: "external-cache" for name in wrappers}
            environment["CARGO_BUILD_BUILD_DIR"] = str(root / "external-artifacts")
            with patch.dict(os.environ, environment), \
                    patch.object(sys, "argv", ["benchmark", "--project", str(project), "--output", str(output), "--command", "check"]), \
                    patch.object(benchmark.subprocess, "check_output", return_value="rustc fixture"), \
                    patch.object(benchmark.subprocess, "run", side_effect=run):
                benchmark.main()
            self.assertEqual(len(captured), 4)
            for env in captured:
                for name in wrappers:
                    self.assertEqual(env.get(name), "", name)
                self.assertEqual(Path(env["CARGO_BUILD_BUILD_DIR"]), output / "target")
            self.assertEqual((project / "src/main.rs").read_text(), "fixture")
            self.assertEqual((project / "src/config.rs").read_text(), "fixture")


if __name__ == "__main__":
    unittest.main()
