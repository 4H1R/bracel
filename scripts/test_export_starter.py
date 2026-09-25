import importlib.util
from pathlib import Path
import tempfile
import tomllib
import unittest

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
        (self.root / "Cargo.lock").write_text("locked")
        self.destination = self.root / "application"
        self.revision = "a" * 40

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
        exporter.export_starter(self.destination, self.revision, self.root)
        exported = tomllib.loads((self.destination / "Cargo.toml").read_text())
        self.assertEqual(exported["dev-dependencies"]["bracel"]["features"], ["testing"])
        self.assertEqual(exported["dev-dependencies"]["bracel"]["rev"], self.revision)
        self.assertTrue(exported["dependencies"]["bracel-integrations"]["optional"])
        self.assertFalse(exported["dependencies"]["bracel-integrations"]["default-features"])


if __name__ == "__main__":
    unittest.main()
