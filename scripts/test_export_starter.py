import importlib.util
from pathlib import Path
import tempfile
import tomllib
import unittest
import subprocess
import json

spec = importlib.util.spec_from_file_location(
    "export_starter", Path(__file__).with_name("export-starter.py"))
exporter = importlib.util.module_from_spec(spec)
spec.loader.exec_module(exporter)


class StarterExportTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        package = '[package]\nname = "example"\nversion = "2.3.4"\n'
        for name in ("bracel", "bracel-cli"):
            path = self.root / "crates" / name
            path.mkdir(parents=True)
            (path / "Cargo.toml").write_text(package)
        self.source = self.root / "starter"
        self.source.mkdir()
        (self.source / "Cargo.toml").write_text(
            package + '[dependencies]\nbracel = { path = "../crates/bracel", version = "2.3.4" }\n')
        (self.source / ".env").write_text("private fixture")
        (self.source / "README.md").write_text("starter")
        (self.root / "Cargo.toml").write_text(
            '[workspace]\nmembers = ["starter"]\n'
            '[profile.release]\nstrip = true\nlto = "thin"\n')
        (self.root / "Cargo.lock").write_text("locked")
        self.destination = self.root / "application"
        self.git("init", "-q")
        self.git("config", "user.name", "Export test")
        self.git("config", "user.email", "export@example.test")
        (self.root / ".gitignore").write_text(".env\n")
        self.commit()

    def git(self, *args):
        return subprocess.check_output(["git", "-C", str(self.root), *args], text=True).strip()

    def commit(self):
        self.git("add", ".")
        self.git("commit", "-qm", "Fixture")
        self.revision = self.git("rev-parse", "HEAD")

    def test_export_pins_version_and_revision_without_local_environment(self):
        exporter.export_starter(self.destination, self.revision, self.root)
        manifest = tomllib.loads((self.destination / "Cargo.toml").read_text())
        self.assertEqual(manifest["dependencies"]["bracel"],
                         {"version": "2.3.4", "git": exporter.REPOSITORY, "rev": self.revision})
        self.assertIn("workspace", manifest)
        self.assertFalse((self.destination / ".env").exists())
        self.assertEqual((self.destination / "Cargo.lock").read_text(), "locked")

    def test_invalid_inputs_leave_no_destination_and_existing_files_intact(self):
        (self.root / "crates/bracel-cli/Cargo.toml").write_text(
            '[package]\nversion = "9.0.0"\n')
        self.commit()
        with self.assertRaisesRegex(ValueError, "versions must match"):
            exporter.export_starter(self.destination, self.revision, self.root)
        self.assertFalse(self.destination.exists())
        with self.assertRaisesRegex(ValueError, "outside"):
            exporter.export_starter(self.source / "nested", self.revision, self.root)
        with self.assertRaisesRegex(ValueError, "commit SHA"):
            exporter.export_starter(self.destination, "main", self.root)
        with self.assertRaisesRegex(ValueError, "must not exist"):
            exporter.export_starter(self.root, self.revision, self.root)
        self.assertEqual((self.source / ".env").read_text(), "private fixture")

    def test_optional_and_test_features_survive_revision_pinning(self):
        integrations = self.root / "crates/bracel-integrations"
        integrations.mkdir()
        (integrations / "Cargo.toml").write_text('[package]\nversion = "2.3.4"\n')
        manifest = self.source / "Cargo.toml"
        manifest.write_text(manifest.read_text() +
            'bracel-integrations = { version = "2.3.4", path = "../crates/bracel-integrations", optional = true, default-features = false }\n'
            '[dev-dependencies]\nbracel = { version = "2.3.4", path = "../crates/bracel", features = ["testing"] }\n')
        self.commit()
        exporter.export_starter(self.destination, self.revision, self.root)
        exported = tomllib.loads((self.destination / "Cargo.toml").read_text())
        self.assertEqual(exported["dev-dependencies"]["bracel"]["features"], ["testing"])
        self.assertEqual(exported["dev-dependencies"]["bracel"]["rev"], self.revision)
        self.assertTrue(exported["dependencies"]["bracel-integrations"]["optional"])
        self.assertFalse(exported["dependencies"]["bracel-integrations"]["default-features"])

    def test_export_uses_committed_revision_not_dirty_worktree(self):
        (self.source / "README.md").write_text("unreleased changes")
        (self.source / "untracked.rs").write_text("unreleased code")
        exporter.export_starter(self.destination, self.revision, self.root)
        self.assertEqual((self.destination / "README.md").read_text(), "starter")
        self.assertFalse((self.destination / "untracked.rs").exists())
        record = json.loads((self.destination / "EXPORT_MANIFEST.json").read_text())
        self.assertEqual(record["source_revision"], self.revision)
        self.assertEqual(record["framework_revision"], self.revision)
        self.assertIn("README.md", record["sha256"])

    def test_unknown_revision_leaves_no_destination(self):
        with self.assertRaises(ValueError):
            exporter.export_starter(self.destination, "f" * 40, self.root)
        self.assertFalse(self.destination.exists())

    def test_export_pins_framework_doc_links_and_preserves_application_links(self):
        docs = self.root / "docs"
        docs.mkdir()
        (docs / "http guide.md").write_text("# Requests\n")
        (self.source / "docs").mkdir()
        (self.source / "docs/operations.md").write_text("# Operations\n")
        readme = ('[HTTP](<../docs/http guide.md#requests>)\n'
                  '[Operations](docs/operations.md)\n'
                  '[External](https://example.test/docs)\n'
                  '[Reference][http]\n\n[http]: ../docs/http%20guide.md#requests\n'
                  '```markdown\n[Example](../docs/example.md)\n```\n')
        (self.source / "README.md").write_text(readme)
        (self.source / "AGENTS.md").write_text('[HTTP](../docs/http%20guide.md)\n')
        self.commit()
        (self.source / "README.md").write_text("dirty instructions")
        exporter.export_starter(self.destination, self.revision, self.root)
        exported = (self.destination / "README.md").read_text()
        pinned = f'{exporter.REPOSITORY}/blob/{self.revision}/docs/http%20guide.md'
        self.assertIn(f'[HTTP](<{pinned}#requests>)', exported)
        self.assertIn(f'[http]: {pinned}#requests', exported)
        self.assertIn('[Operations](docs/operations.md)', exported)
        self.assertIn('[External](https://example.test/docs)', exported)
        self.assertIn('[Example](../docs/example.md)', exported)
        self.assertEqual((self.destination / "AGENTS.md").read_text(), f'[HTTP]({pinned})\n')
        self.assertFalse((self.destination / "docs/http guide.md").exists())
        record = json.loads((self.destination / "EXPORT_MANIFEST.json").read_text())
        import hashlib
        self.assertEqual(record['sha256']['README.md'],
                         hashlib.sha256((self.destination / 'README.md').read_bytes()).hexdigest())

    def test_export_preserves_all_committed_workspace_profiles(self):
        workspace = self.root / "Cargo.toml"
        workspace.write_text(workspace.read_text() +
            '[profile.dev]\ndebug = "line-tables-only"\n'
            '[profile.dev-full]\ninherits = "dev"\ndebug = 2\n'
            '[profile.release-fast]\ninherits = "release"\nopt-level = 2\n'
            'lto = false\nincremental = true\n'
            '[profile.release-fast.build-override]\ncodegen-units = 128\n'
            '[profile.dev.package."example-dependency:1.2.3"]\nopt-level = 1\n'
            '[workspace.metadata]\nnote = "unrelated"\n')
        expected = tomllib.loads(workspace.read_text())["profile"]
        self.commit()
        workspace.write_text('[workspace]\n[profile.dev]\ndebug = false\n')
        exporter.export_starter(self.destination, self.revision, self.root)
        exported = tomllib.loads((self.destination / "Cargo.toml").read_text())
        self.assertEqual(exported["profile"], expected)
        self.assertEqual(exported["workspace"], {})

    def test_export_keeps_cargo_defaults_when_workspace_has_no_profiles(self):
        (self.root / "Cargo.toml").write_text('[workspace]\nmembers = ["starter"]\n')
        self.commit()
        exporter.export_starter(self.destination, self.revision, self.root)
        exported = tomllib.loads((self.destination / "Cargo.toml").read_text())
        self.assertNotIn("profile", exported)

    def test_reference_starter_cannot_override_workspace_profiles(self):
        manifest = self.source / "Cargo.toml"
        manifest.write_text(manifest.read_text() + '\n[profile.dev]\ndebug = 2\n')
        self.commit()
        with self.assertRaisesRegex(ValueError, "inherit the workspace build profiles"):
            exporter.export_starter(self.destination, self.revision, self.root)
        self.assertFalse(self.destination.exists())

    def test_generated_files_use_lf_line_endings(self):
        exporter.export_starter(self.destination, self.revision, self.root)
        for name in ("Cargo.toml", "STARTER_VERSION", "EXPORT_MANIFEST.json"):
            with self.subTest(file=name):
                content = (self.destination / name).read_bytes()
                self.assertIn(b"\n", content)
                self.assertNotIn(b"\r\n", content)

    def test_utf8_manifest_comments_survive_export(self):
        path = self.source / "Cargo.toml"
        comment = "# Caf\u00e9 / \u0641\u0631\u064a\u0642\n"
        path.write_text(comment + path.read_text(encoding="utf-8"),
                        encoding="utf-8", newline="\n")
        self.commit()
        exporter.export_starter(self.destination, self.revision, self.root)
        self.assertTrue((self.destination / "Cargo.toml").read_bytes()
                        .startswith(comment.encode("utf-8")))


if __name__ == "__main__":
    unittest.main()
