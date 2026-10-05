"""SemVer contracts; exhaustive/tool fixtures are separate from routine checks."""
import importlib.machinery
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import shutil
import tarfile
import tempfile
import unittest
import sys
sys.dont_write_bytecode = True
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
loader = importlib.machinery.SourceFileLoader("semver_intent", str(ROOT / ".local/bin/semver-intent"))
spec = importlib.util.spec_from_loader(loader.name, loader)
semver = importlib.util.module_from_spec(spec)
loader.exec_module(semver)


class ApiFixture:
    """A self-contained packaged library pair with a declared intent."""

    def __init__(self, root, old_api="pub fn original() {}", new_api="pub fn original() {}", proc_macro=False):
        self.old, self.new = root / "old", root / "new"
        for directory, api in ((self.old, old_api), (self.new, new_api)):
            source = directory / "packages" / "fixture"
            (source / "src").mkdir(parents=True)
            manifest = '[package]\nname = "fixture"\nversion = "1.0.0"\nedition = "2021"\n'
            if proc_macro:
                manifest += '\n[lib]\nproc-macro = true\n'
            (source / "Cargo.toml").write_text(manifest)
            (source / "src/lib.rs").write_text(api)
        self.manifest = {"schema": semver.SCHEMA, "target": "x86_64-unknown-linux-gnu", "features": [],
                         "crates": [{"name": "fixture", "version": "1.0.0", "library": True, "proc_macro": proc_macro, "lib_name": "fixture"}]}

    def level(self):
        return semver.detected_level(self.new, self.old, "fixture", self.manifest["target"], [], self.manifest["crates"][0]["proc_macro"])


class IntentTests(unittest.TestCase):
    def test_complete_level_matrix(self):
        with tempfile.TemporaryDirectory() as temporary:
            fixture = ApiFixture(Path(temporary))
            for required in semver.LEVELS:
                for declared in semver.LEVELS:
                    with self.subTest(required=required, declared=declared), patch.object(semver, "detected_level", return_value=required):
                        result = semver.compare(fixture.new, fixture.manifest, fixture.old, fixture.manifest, {"fixture": declared})
                        self.assertEqual(result, int(semver.LEVELS.index(required) > semver.LEVELS.index(declared)))

    def test_analysis_error_is_never_major_or_success(self):
        with patch.object(semver, "api_rustdoc", return_value=Path("api.json")), patch.object(semver.subprocess, "run", return_value=subprocess.CompletedProcess([], 101)):
            with self.assertRaisesRegex(semver.CheckError, "analysis failed"):
                semver.detected_level(Path("new"), Path("old"), "fixture", "target", [])

    def test_unexpected_exit_is_error(self):
        with patch.object(semver, "api_rustdoc", return_value=Path("api.json")), patch.object(semver.subprocess, "run", return_value=subprocess.CompletedProcess([], 1)):
            with self.assertRaises(semver.CheckError):
                semver.detected_level(Path("new"), Path("old"), "fixture", "target", [])

    def test_publishable_selection_uses_workspace_members(self):
        package = lambda name, publish=None: {"name": name, "id": name, "source": None, "publish": publish}
        data = {"packages": [package("selected"), package("private", []), package("outside")],
                "workspace_members": ["selected", "private"], "metadata": {"next-release": {"selected": "patch"}}}
        with patch.object(semver, "run", return_value=json.dumps(data)):
            packages, intent = semver.metadata(Path("source"))
            self.assertEqual([p["name"] for p in packages], ["selected"])
            for invalid in (None, "automatic", "", 3):
                data["metadata"]["next-release"]["selected"] = invalid
                with self.subTest(invalid=invalid), patch.object(semver, "run", return_value=json.dumps(data)):
                    with self.assertRaisesRegex(semver.CheckError, "missing or invalid"):
                        semver.metadata(Path("source"))

    def test_removed_crate_requires_explicit_transition(self):
        with self.assertRaisesRegex(semver.CheckError, "removed"):
            semver.compare(Path("new"), {"crates": []}, Path("old"), {"crates": [{"name": "removed"}]}, {})

    def test_missing_baseline_does_not_bootstrap(self):
        with tempfile.TemporaryDirectory() as temporary:
            with self.assertRaisesRegex(semver.CheckError, "explicitly approved revision"):
                semver.baseline_root(Path(temporary) / "missing", Path(temporary), "target", [])

    def test_corrupt_and_escaping_archives_are_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = root / "bad.tar.gz"
            archive.write_bytes(b"invalid")
            with self.assertRaises(tarfile.TarError):
                semver.unpack(archive, root / "output")
            with tarfile.open(archive, "w:gz") as bundle:
                member = tarfile.TarInfo("../escape")
                member.size = 1
                bundle.addfile(member, io.BytesIO(b"x"))
            with self.assertRaises(tarfile.TarError):
                semver.unpack(archive, root / "output")
            self.assertFalse((root / "escape").exists())

    def test_non_object_baseline_is_invalid_input(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "manifest.json").write_text("[]")
            with self.assertRaisesRegex(semver.CheckError, "must be an object"):
                semver.validate_manifest(root, "target", [])

    def test_scope_and_schema_must_match(self):
        with tempfile.TemporaryDirectory() as temporary:
            fixture = ApiFixture(Path(temporary))
            (fixture.old / "manifest.json").write_text(json.dumps(fixture.manifest))
            for target, features in (("different-target", []), (fixture.manifest["target"], ["sqlite"])):
                with self.subTest(target=target, features=features):
                    with self.assertRaisesRegex(semver.CheckError, "target/features"):
                        semver.validate_manifest(fixture.old, target, features)
            fixture.manifest["schema"] = -1
            (fixture.old / "manifest.json").write_text(json.dumps(fixture.manifest))
            with self.assertRaisesRegex(semver.CheckError, "schema"):
                semver.validate_manifest(fixture.old, fixture.manifest["target"], [])

    def test_snapshot_does_not_replace_baseline_and_stable_does(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            candidate, destination = root / "candidate", root / "active"
            candidate.write_bytes(b"new trusted artifact")
            destination.write_bytes(b"old")
            args = [str(ROOT / ".local/bin/adopt-semver-baseline"), "--candidate", str(candidate), "--destination", str(destination), "--release-tag"]
            subprocess.run(args + ["v1.0.0-snapshot1"], check=True)
            self.assertEqual(destination.read_bytes(), b"old")
            subprocess.run(args + ["invalid"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            self.assertEqual(destination.read_bytes(), b"old")
            subprocess.run(args + ["v1.0.0"], check=True)
            self.assertEqual(destination.read_bytes(), candidate.read_bytes())


class PackagingTests(unittest.TestCase):
    def test_packaged_inputs_and_cached_local_check(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source, cache = root / "source", root / "cache"
            source.mkdir()
            cache.mkdir()
            helpers = source / ".local/bin"
            helpers.mkdir(parents=True)
            for name in ("materialize-release-versions", "bump-version"):
                shutil.copy2(ROOT / ".local/bin" / name, helpers / name)
            (source / "src").mkdir()
            (source / "src/lib.rs").write_text("pub fn original() -> u32 { semver_intent_fixture_dependency::value() }")
            dependency = source / "dependency"
            (dependency / "src").mkdir(parents=True)
            (dependency / "src/lib.rs").write_text("pub fn value() -> u32 { 7 }")
            (dependency / "Cargo.toml").write_text(
                '[package]\nname = "semver-intent-fixture-dependency"\nversion = "1.0.0"\nedition = "2021"\n')
            (source / "src/excluded.rs").write_text("not valid Rust")
            (source / "Cargo.toml").write_text(
                '[package]\nname = "fixture"\nversion = "1.0.0"\nedition = "2021"\n'
                'exclude = ["src/excluded.rs"]\n[dependencies]\n'
                'semver-intent-fixture-dependency = { path = "dependency", version = "1.0.0" }\n'
                '[workspace]\nmembers = ["dependency"]\n'
                '[workspace.metadata.next-release]\nfixture = "patch"\nsemver-intent-fixture-dependency = "patch"\n')
            subprocess.run(["git", "init", "-q", str(source)], check=True)
            subprocess.run(["git", "-C", str(source), "add", "."], check=True)
            subprocess.run(["git", "-C", str(source), "-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid", "commit", "-qm", "test: fixture"], check=True)
            original_manifest = (source / "Cargo.toml").read_bytes()
            packaged, manifest, intent = semver.prepare(source, cache, "x86_64-unknown-linux-gnu", [])
            self.assertEqual(manifest["crates"][0]["version"], "1.0.1")
            self.assertFalse((packaged / "packages/fixture/src/excluded.rs").exists())
            self.assertEqual((source / "Cargo.toml").read_bytes(), original_manifest)
            # A second routine invocation must not materialize or package again.
            real_run = semver.run
            def cached_run(args, *arguments):
                if args[:2] == ["cargo", "package"] or args[0] == "bash":
                    self.fail("cache miss repeated packaging/materialization")
                return real_run(args, *arguments)
            with patch.object(semver, "run", side_effect=cached_run):
                reused, _, _ = semver.prepare(source, cache, "x86_64-unknown-linux-gnu", [])
                self.assertEqual(reused, packaged)
            self.assertEqual(semver.compare(packaged, manifest, packaged, manifest, intent), 0)


class ToolFixtureTests(unittest.TestCase):
    def test_real_api_thresholds(self):
        # Additions are not generally reported by this tool; do not claim otherwise.
        cases = (("unchanged", "pub fn original() {}", "pub fn original() {}", "patch"),
                 ("addition", "pub fn original() {}", "pub fn original() {} pub fn added() {}", "patch"),
                 ("minor", "pub struct Thing;", "#[must_use] pub struct Thing;", "minor"),
                 ("major", "pub fn original() {}", "pub fn renamed() {}", "major"))
        for name, old, new, expected in cases:
            with self.subTest(name=name), tempfile.TemporaryDirectory() as temporary:
                fixture = ApiFixture(Path(temporary), old, new)
                self.assertEqual(fixture.level(), expected)

    def test_json_cache_reuse_and_environment_invalidation(self):
        with tempfile.TemporaryDirectory() as temporary:
            fixture = ApiFixture(Path(temporary))
            self.assertEqual(fixture.level(), "patch")
            real_run = subprocess.run
            builds = []
            def track_run(args, **kwargs):
                if args[:2] == ["cargo", "rustdoc"]:
                    builds.append(args)
                return real_run(args, **kwargs)
            with patch.object(semver.subprocess, "run", side_effect=track_run):
                self.assertEqual(fixture.level(), "patch")
                self.assertEqual(builds, [])
                with patch.dict(os.environ, {"SEMVER_FIXTURE_CONTEXT": "changed"}):
                    self.assertEqual(fixture.level(), "patch")
                    self.assertEqual(len(builds), 2)

    def test_real_macro_removal(self):
        macro = 'extern crate proc_macro; #[proc_macro] pub fn original(_: proc_macro::TokenStream) -> proc_macro::TokenStream { proc_macro::TokenStream::new() }'
        with tempfile.TemporaryDirectory() as temporary:
            fixture = ApiFixture(Path(temporary), macro, macro.replace("original", "renamed"), proc_macro=True)
            self.assertEqual(fixture.level(), "major")

    def test_real_compile_error_is_analysis_error(self):
        with tempfile.TemporaryDirectory() as temporary:
            fixture = ApiFixture(Path(temporary), new_api="this is not Rust")
            with self.assertRaises(semver.CheckError):
                fixture.level()


if __name__ == "__main__":
    unittest.main()
