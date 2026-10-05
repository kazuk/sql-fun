"""SemVer contracts; exhaustive/tool fixtures are separate from routine checks."""
import io
import importlib.machinery
import importlib.util
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
sys.path.insert(0, str(ROOT / ".local/lib"))
from semver_intent import analysis as semver
from semver_intent import commands, packages, storage
from semver_intent.model import SCHEMA


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
        self.manifest = {"schema": SCHEMA, "target": "x86_64-unknown-linux-gnu", "features": [],
                         "crates": [{"name": "fixture", "version": "1.0.0", "library": True, "proc_macro": proc_macro, "lib_name": "fixture"}]}

    def level(self):
        return semver.detected_level(self.new, self.old, "fixture", self.manifest["target"], [], self.manifest["crates"][0]["proc_macro"])


class IntentTests(unittest.TestCase):
    def test_feature_scope_must_be_explicit_before_reading_source(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            result = subprocess.run([str(ROOT / ".local/bin/semver-intent"), "check",
                                     "--source", str(root / "unselected-source"), "--target", "target",
                                     "--cache", str(root / "cache"), "--baseline", str(root / "baseline")],
                                    text=True, capture_output=True)
            self.assertEqual(result.returncode, 2)
            self.assertIn("--default-features", result.stderr)
            self.assertFalse((root / "cache").exists())

    def test_complete_level_matrix(self):
        with tempfile.TemporaryDirectory() as temporary:
            fixture = ApiFixture(Path(temporary))
            for required in semver.LEVELS:
                for declared in semver.LEVELS:
                    with self.subTest(required=required, declared=declared), patch.object(semver, "detected_level", return_value=required):
                        result = semver.compare(fixture.new, fixture.manifest, fixture.old, fixture.manifest, {"fixture": declared})
                        self.assertEqual(result, int(semver.LEVELS.index(required) > semver.LEVELS.index(declared)))

    def test_analysis_error_is_never_major_or_success(self):
        with patch.object(semver, "api_rustdoc", return_value=Path("api.json")), patch.object(commands, "semver_probe", return_value=101):
            with self.assertRaisesRegex(semver.CheckError, "analysis failed"):
                semver.detected_level(Path("new"), Path("old"), "fixture", "target", [])

    def test_unexpected_exit_is_error(self):
        with patch.object(semver, "api_rustdoc", return_value=Path("api.json")), patch.object(commands, "semver_probe", return_value=1):
            with self.assertRaises(semver.CheckError):
                semver.detected_level(Path("new"), Path("old"), "fixture", "target", [])

    def test_publishable_selection_uses_workspace_members(self):
        package = lambda name, publish=None: {"name": name, "id": name, "source": None, "publish": publish}
        data = {"packages": [package("selected"), package("private", []), package("outside")],
                "workspace_members": ["selected", "private"], "metadata": {"next-release": {"selected": "patch"}}}
        with patch.object(commands, "run", return_value=json.dumps(data)):
            selected, intent = packages.metadata(Path("source"))
            self.assertEqual([p["name"] for p in selected], ["selected"])
            for invalid in (None, "automatic", "", 3):
                data["metadata"]["next-release"]["selected"] = invalid
                with self.subTest(invalid=invalid), patch.object(commands, "run", return_value=json.dumps(data)):
                    with self.assertRaisesRegex(semver.CheckError, "missing or invalid"):
                        packages.metadata(Path("source"))

    def test_removed_crate_requires_explicit_transition(self):
        with self.assertRaisesRegex(semver.CheckError, "removed"):
            semver.compare(Path("new"), {"crates": []}, Path("old"), {"crates": [{"name": "removed"}]}, {})

    def test_missing_baseline_does_not_bootstrap(self):
        with tempfile.TemporaryDirectory() as temporary:
            with self.assertRaisesRegex(semver.CheckError, "explicitly approved revision"):
                packages.baseline_root(Path(temporary) / "missing", Path(temporary), "target", [])

    def test_corrupt_and_escaping_archives_are_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = root / "bad.tar.gz"
            archive.write_bytes(b"invalid")
            with self.assertRaises(tarfile.TarError):
                storage.unpack(archive, root / "output")
            with tarfile.open(archive, "w:gz") as bundle:
                member = tarfile.TarInfo("../escape")
                member.size = 1
                bundle.addfile(member, io.BytesIO(b"x"))
            with self.assertRaises(tarfile.TarError):
                storage.unpack(archive, root / "output")
            self.assertFalse((root / "escape").exists())

    def test_non_object_baseline_is_invalid_input(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "manifest.json").write_text("[]")
            with self.assertRaisesRegex(semver.CheckError, "must be an object"):
                packages.validate_manifest(root, "target", [])

    def test_scope_and_schema_must_match(self):
        with tempfile.TemporaryDirectory() as temporary:
            fixture = ApiFixture(Path(temporary))
            (fixture.old / "manifest.json").write_text(json.dumps(fixture.manifest))
            for target, features in (("different-target", []), (fixture.manifest["target"], ["sqlite"])):
                with self.subTest(target=target, features=features):
                    with self.assertRaisesRegex(semver.CheckError, "target/features"):
                        packages.validate_manifest(fixture.old, target, features)
            fixture.manifest["schema"] = -1
            (fixture.old / "manifest.json").write_text(json.dumps(fixture.manifest))
            with self.assertRaisesRegex(semver.CheckError, "schema"):
                packages.validate_manifest(fixture.old, fixture.manifest["target"], [])

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


class ReleaseFixture:
    """A selected repository releasing a library and its unpublished dependency."""

    def __init__(self, root):
        self.source, self.cache = root / "source", root / "cache"
        source, cache = self.source, self.cache
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
        (source / "README.md").write_text("# Fixture\n")
        subprocess.run(["git", "init", "-q", str(source)], check=True)
        subprocess.run(["git", "-C", str(source), "add", "."], check=True)
        subprocess.run(["git", "-C", str(source), "-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid", "commit", "-qm", "test: fixture"], check=True)

    def prepare(self, *, candidate=False):
        return packages.prepare(self.source, self.cache, "x86_64-unknown-linux-gnu", [], candidate=candidate)


class LocalPolicyTests(unittest.TestCase):
    def test_missing_tool_skips_locally_and_fails_when_required(self):
        with tempfile.TemporaryDirectory() as temporary:
            binary = Path(temporary) / "git"
            binary.symlink_to(shutil.which("git"))
            environment = dict(os.environ, PATH=temporary)
            args = ["/bin/bash", str(ROOT / ".local/bin/check-test-flow"), "--step=semver"]
            local = subprocess.run(args, cwd=ROOT, env=environment, text=True, capture_output=True)
            self.assertEqual(local.returncode, 0)
            self.assertIn("Skipping optional local SemVer", local.stdout)
            required = subprocess.run(args + ["--require-semver"], cwd=ROOT, env=environment,
                                      text=True, capture_output=True)
            self.assertNotEqual(required.returncode, 0)
            self.assertIn("cargo-semver-checks required", required.stdout)

    def test_local_tool_version_is_not_an_exact_requirement(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            fixture = ApiFixture(root)
            plugin = root / "cargo-semver-checks"
            actual = shutil.which("cargo-semver-checks")
            plugin.write_text('#!/bin/sh\nif [ "$2" = "--version" ]; then\n'
                              'echo "cargo-semver-checks 0.50.0"\nelse\n'
                              f'exec "{actual}" "$@"\nfi\n')
            plugin.chmod(0o755)
            loader = importlib.machinery.SourceFileLoader("semver_cli", str(ROOT / ".local/bin/semver-intent"))
            cli = importlib.util.module_from_spec(importlib.util.spec_from_loader(loader.name, loader))
            loader.exec_module(cli)
            args = ["semver-intent", "check", "--source", str(root), "--cache", str(root / "cache"),
                    "--baseline", str(root / "baseline"), "--target", fixture.manifest["target"], "--default-features"]
            environment = {"PATH": str(root) + os.pathsep + os.environ["PATH"]}
            with patch.dict(os.environ, environment), patch.object(sys, "argv", args), \
                    patch.object(packages, "baseline_root", return_value=(fixture.old, fixture.manifest)), \
                    patch.object(packages, "prepare", return_value=(fixture.new, fixture.manifest, {"fixture": "patch"})):
                self.assertEqual(cli.main(), 0)
                with patch.object(sys, "argv", args + ["--require-tool-version", "0.51.0"]):
                    with self.assertRaisesRegex(semver.CheckError, "CI requires"):
                        cli.main()


class PackagingTests(unittest.TestCase):
    def test_packaged_inputs_and_cached_local_check(self):
        with tempfile.TemporaryDirectory() as temporary:
            fixture = ReleaseFixture(Path(temporary))
            source, cache = fixture.source, fixture.cache
            original_manifest = (source / "Cargo.toml").read_bytes()
            packaged, manifest, intent = fixture.prepare()
            self.assertEqual(manifest["crates"][0]["version"], "1.0.1")
            self.assertFalse((packaged / "packages/fixture/src/excluded.rs").exists())
            self.assertEqual((source / "Cargo.toml").read_bytes(), original_manifest)
            # A second routine invocation must not materialize or package again.
            real_run = commands.run
            def cached_run(args, *arguments, **kwargs):
                if (args[:2] == ["cargo", "package"] and "--list" not in args) or args[0] == "bash":
                    self.fail("cache miss repeated packaging/materialization")
                return real_run(args, *arguments, **kwargs)
            with patch.object(commands, "run", side_effect=cached_run):
                reused, _, _ = fixture.prepare()
                self.assertEqual(reused, packaged)
            self.assertEqual(semver.compare(packaged, manifest, packaged, manifest, intent), 0)

    def test_non_api_edits_reuse_local_analysis_but_refresh_candidates(self):
        with tempfile.TemporaryDirectory() as temporary:
            fixture = ReleaseFixture(Path(temporary))
            baseline, previous, _ = fixture.prepare(candidate=True)
            current, manifest, intent = fixture.prepare()
            self.assertEqual(semver.compare(current, manifest, baseline, previous, intent), 0)
            edits = {"README.md": "# Updated documentation\n",
                     "docs/guide.md": "# Contributor guide\n",
                     ".github/workflows/example.yml": "name: Updated workflow\n",
                     "stats/local.txt": "Unrelated report\n",
                     "src/excluded.rs": "still not a package input"}
            for name, text in edits.items():
                path = fixture.source / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(text)
            with patch.object(packages, "build_packaged_snapshot", side_effect=AssertionError("repackaged non-API edits")), \
                    patch.object(commands, "generate_rustdoc", side_effect=AssertionError("rebuilt non-API edits")):
                reused, manifest, intent = fixture.prepare()
                self.assertEqual(semver.compare(reused, manifest, baseline, previous, intent), 0)
            candidate, _, _ = fixture.prepare(candidate=True)
            self.assertEqual((candidate / "packages/fixture/README.md").read_text(), edits["README.md"])
            (fixture.source / "src/lib.rs").write_text("pub fn renamed() -> u32 { 7 }")
            changed, manifest, intent = fixture.prepare()
            self.assertEqual(semver.compare(changed, manifest, baseline, previous, intent), 1)

    def test_build_script_documentation_inputs_refresh_generated_api(self):
        with tempfile.TemporaryDirectory() as temporary:
            fixture = ReleaseFixture(Path(temporary))
            source = fixture.source
            (source / "build.rs").write_text(
                'fn main() { let name = if std::fs::read_to_string("README.md").unwrap().contains("Changed") '
                '{ "renamed" } else { "original" }; '
                'let output = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("api.rs"); '
                'std::fs::write(output, format!("pub fn {}() {{}}", name)).unwrap(); }')
            (source / "src/lib.rs").write_text('include!(concat!(env!("OUT_DIR"), "/api.rs"));')
            baseline, previous, _ = fixture.prepare(candidate=True)
            first, manifest, intent = fixture.prepare()
            self.assertEqual(semver.compare(first, manifest, baseline, previous, intent), 0)
            (source / "README.md").write_text("# Changed generator input\n")
            changed, manifest, intent = fixture.prepare()
            self.assertEqual(semver.compare(changed, manifest, baseline, previous, intent), 1)

    def test_rust_api_source_in_documentation_directory_is_not_skipped(self):
        with tempfile.TemporaryDirectory() as temporary:
            fixture = ReleaseFixture(Path(temporary))
            module = fixture.source / "docs/api.rs"
            module.parent.mkdir()
            module.write_text("pub fn original() {}")
            (fixture.source / "src/lib.rs").write_text('#[path = "../docs/api.rs"] pub mod exposed;')
            baseline, previous, _ = fixture.prepare(candidate=True)
            current, manifest, intent = fixture.prepare()
            self.assertEqual(semver.compare(current, manifest, baseline, previous, intent), 0)
            module.write_text("pub fn renamed() {}")
            changed, manifest, intent = fixture.prepare()
            self.assertEqual(semver.compare(changed, manifest, baseline, previous, intent), 1)

    def test_embedded_non_markdown_assets_invalidate_analysis(self):
        with tempfile.TemporaryDirectory() as temporary:
            fixture = ReleaseFixture(Path(temporary))
            data = fixture.source / "src/value.txt"
            data.write_text("first")
            (fixture.source / "src/lib.rs").write_text('pub const TEXT: &str = include_str!("value.txt");')
            first, _, _ = fixture.prepare()
            data.write_text("second")
            changed, _, _ = fixture.prepare()
            self.assertEqual((changed / "packages/fixture/src/value.txt").read_text(), "second")
            self.assertNotEqual(first, changed)


class WorkflowGateTests(unittest.TestCase):
    def test_aggregate_rejects_unsuccessful_dependencies(self):
        workflow = (ROOT / ".github/workflows/build-test.yml").read_text()
        aggregate = workflow.split("\n  build:\n", 1)[1]
        self.assertIn("if: ${{ always() }}", aggregate)
        self.assertIn("      - semver\n", aggregate)
        script = "\n".join(line[10:] for line in aggregate.split("        run: |\n", 1)[1].splitlines())
        for status in ("success", "failure", "cancelled", "skipped"):
            with self.subTest(status=status):
                result = subprocess.run(["bash", "-c", script],
                                        env=dict(os.environ, DEPENDENCY_RESULTS="success " + status),
                                        text=True, capture_output=True)
                self.assertEqual(result.returncode == 0, status == "success")


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
            with patch.object(commands.subprocess, "run", side_effect=track_run):
                self.assertEqual(fixture.level(), "patch")
                self.assertEqual(builds, [])
                with patch.dict(os.environ, {"SEMVER_FIXTURE_CONTEXT": "changed"}):
                    self.assertEqual(fixture.level(), "patch")
                    self.assertEqual(len(builds), 2)

    def test_cargo_configuration_invalidates_cached_apis(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            fixture = ApiFixture(root, old_api="pub fn original() {} #[cfg(semver_fixture)] pub fn removed() {}")
            self.assertEqual(fixture.level(), "patch")
            config = root / ".cargo/config.toml"
            config.parent.mkdir()
            config.write_text('[build]\nrustdocflags = ["--cfg", "semver_fixture"]\n')
            self.assertEqual(fixture.level(), "major")

    def test_pre_one_version_bump_does_not_override_detected_impact(self):
        with tempfile.TemporaryDirectory() as temporary:
            fixture = ApiFixture(Path(temporary), new_api="pub fn renamed() {}")
            for root, version in ((fixture.old, "0.1.0"), (fixture.new, "0.2.0")):
                manifest = root / "packages/fixture/Cargo.toml"
                manifest.write_text(manifest.read_text().replace("1.0.0", version))
            self.assertEqual(fixture.level(), "major")

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
