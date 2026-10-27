"""Real package/JSON/analyzer contracts; invoked by xbuild's integration test."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.dont_write_bytecode = True
sys.path.insert(0, str(ROOT / '.local/sbin/package-api'))
from compare import AnalysisError, parse_semver, validate_json


def fingerprint(directory):
    return {str(p.relative_to(directory)): hashlib.sha256(p.read_bytes()).hexdigest()
            for p in directory.rglob('*') if p.is_file()}


class Comparison(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        prepared = subprocess.run([str(ROOT / '.local/sbin/prepare-package-api-tests')],
                                  text=True, capture_output=True, check=True)
        cls.tools = json.loads(prepared.stdout)

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix='package-api-fixture-')
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)

    def package(self, side, source, features='', proc_macro=False, name='probe', dependencies=''):
        path = self.root / side
        (path / 'src').mkdir(parents=True)
        (path / 'Cargo.toml').write_text(
            f'[package]\nname={json.dumps(name)}\nversion="1.0.0"\nedition="2021"\n'
            f'[lib]\nproc-macro={str(proc_macro).lower()}\n[features]\n{features}\n'
            f'[dependencies]\n{dependencies}\n')
        (path / 'src/lib.rs').write_text(source)
        return dict(package=str(path), dependencies=[], features=[], default_features=False)

    def compare(self, old, new, expected='detected', analyzer=None, **overrides):
        request = dict(old=old, new=new, target='x86_64-unknown-linux-gnu', toolchain=self.tools['toolchain'])
        request.update(overrides)
        path = self.root / 'request.json'
        path.write_text(json.dumps(request))
        inputs = {Path(request[s]['package']) for s in ('old', 'new')}
        inputs.update(Path(p) for s in ('old', 'new') for p in request[s]['dependencies'])
        before = {p: fingerprint(p) for p in inputs}
        result = subprocess.run([sys.executable, str(ROOT / '.local/sbin/compare-package-api'), '--request', str(path),
                                 '--diff-wrapper', os.environ['PACKAGE_API_DIFF_WRAPPER'],
                                 '--semver-checks', str(analyzer or self.tools['semver_checks'])], text=True, capture_output=True)
        report = json.loads(result.stdout)
        self.assertEqual(report['status'], expected, report)
        self.assertEqual(result.returncode, 0 if expected == 'detected' else 1, report)
        for package, contents in before.items():
            self.assertEqual(fingerprint(package), contents)
        return report

    def test_unchanged(self):
        report = self.compare(self.package('old', 'pub fn api() {}'), self.package('new', 'pub fn api() {}'))
        self.assertEqual(report['impact'], 'patch')
        self.assertTrue(report['limits'])

    def test_addition(self):
        report = self.compare(self.package('old', ''), self.package('new', 'pub fn added() {}'))
        self.assertEqual(report['impact'], 'minor')
        self.assertEqual(report['diff']['added'], ['pub fn probe::added()'])

    def test_real_cargo_package_contents(self):
        sides = []
        for side, source in (('old', 'pub fn api() {}'), ('new', 'pub fn api() {} pub fn added() {}')):
            selection = self.package(side, source)
            directory = Path(selection['package'])
            subprocess.run(['cargo', 'package', '--no-verify', '--offline', '--allow-dirty',
                            '--manifest-path', str(directory / 'Cargo.toml'),
                            '--target-dir', str(directory / 'target')],
                           cwd=directory, check=True, capture_output=True)
            unpacked = self.root / (side + '-unpacked')
            with tarfile.open(directory / 'target/package/probe-1.0.0.crate') as archive:
                archive.extractall(unpacked, filter='data')
            selection['package'] = str(unpacked / 'probe-1.0.0')
            sides.append(selection)
        self.assertEqual(self.compare(*sides)['impact'], 'minor')

    def test_removal(self):
        report = self.compare(self.package('old', 'pub fn api() {}'), self.package('new', ''))
        self.assertEqual(report['impact'], 'major')
        self.assertGreater(report['semver']['major'], 0)

    def test_signature(self):
        report = self.compare(self.package('old', 'pub fn api() {}'), self.package('new', 'pub fn api(_: u8) {}'))
        self.assertEqual(report['impact'], 'major')
        self.assertTrue(report['diff']['changed'])

    def test_conservative_changed_item_without_semver_finding(self):
        old = self.package('old', 'pub fn api<T>(x: T) {}')
        new = self.package('new', 'pub fn api<U>(x: U) {}')
        report = self.compare(old, new)
        self.assertEqual(report['impact'], 'major')
        self.assertTrue(report['diff']['changed'])
        self.assertEqual(report['semver']['major'], 0)

    def test_deprecation_without_structural_diff(self):
        report = self.compare(self.package('old', 'pub fn api() {}'), self.package('new', '#[deprecated] pub fn api() {}'))
        self.assertEqual(report['impact'], 'minor')
        self.assertFalse(report['diff']['changed'])
        self.assertGreater(report['semver']['minor'], 0)

    def test_required_trait_method(self):
        report = self.compare(self.package('old', 'pub trait Api {}'), self.package('new', 'pub trait Api { fn run(&self); }'))
        self.assertTrue(report['diff']['added'])
        self.assertEqual(report['impact'], 'major')

    def test_exhaustive_enum_variant(self):
        report = self.compare(self.package('old', 'pub enum Api { A }'), self.package('new', 'pub enum Api { A, B }'))
        self.assertTrue(report['diff']['added'])
        self.assertEqual(report['impact'], 'major')

    def test_zero_exit_major_warning(self):
        old = self.package('old', '#[derive(PartialEq, PartialOrd)] pub struct Api { pub a: u8, pub b: u8 }')
        new = self.package('new', '#[derive(PartialEq, PartialOrd)] pub struct Api { pub b: u8, pub a: u8 }')
        report = self.compare(old, new)
        self.assertEqual(report['impact'], 'major')
        self.assertEqual(report['semver']['exit_status'], 0)
        self.assertEqual(report['semver']['major'], 1)
        self.assertFalse(report['diff']['changed'])

    def test_candidate_only_feature(self):
        old = self.package('old', 'pub fn api() {}')
        new = self.package('new', 'pub fn api() {} #[cfg(feature="extra")] pub fn added() {}', 'extra=[]')
        new['features'] = ['extra']
        report = self.compare(old, new)
        self.assertEqual(report['impact'], 'minor')
        self.assertEqual(report['coverage']['new']['resolved_features'], ['extra'])

    def test_new_coverage_does_not_hide_removal(self):
        old = self.package('old', 'pub fn api() {}')
        new = self.package('new', '#[cfg(feature="extra")] pub fn added() {}', 'extra=[]')
        new['features'] = ['extra']
        report = self.compare(old, new)
        self.assertEqual(report['impact'], 'major')
        self.assertTrue(report['diff']['removed'])
        self.assertTrue(report['diff']['added'])

    def test_independent_defaults(self):
        source = '#[cfg(feature="extra")] pub fn api() {}'
        old = self.package('old', source, 'default=["extra"]\nextra=[]')
        new = self.package('new', source, 'default=["extra"]\nextra=[]')
        old['default_features'] = True
        report = self.compare(old, new)
        self.assertEqual(report['impact'], 'major')
        self.assertIn('extra', report['coverage']['old']['resolved_features'])
        self.assertEqual(report['coverage']['new']['resolved_features'], [])

    def test_selected_feature_removal(self):
        old = self.package('old', '#[cfg(feature="extra")] pub fn api() {}', 'extra=[]')
        new = self.package('new', '', 'extra=[]')
        old['features'] = new['features'] = ['extra']
        self.assertEqual(self.compare(old, new)['impact'], 'major')

    def test_disabled_surface(self):
        old = self.package('old', '#[cfg(feature="extra")] pub fn api() {}', 'extra=[]')
        new = self.package('new', '', 'extra=[]')
        self.assertEqual(self.compare(old, new)['impact'], 'patch')

    def test_inventory_only_removal(self):
        report = self.compare(self.package('old', '', 'extra=[]'), self.package('new', ''))
        self.assertEqual(report['removed_features'], ['extra'])
        self.assertEqual(report['impact'], 'major')
        self.assertEqual(report['semver']['major'], 0)

    def test_macro_add_remove(self):
        # Each macro kind must go through production JSON generation, not
        # the source-based SemVer path which excludes proc-macro targets.
        for kind, source in {
            'function': '#[proc_macro] pub fn api(x: proc_macro::TokenStream) -> proc_macro::TokenStream { x }',
            'derive': '#[proc_macro_derive(Api)] pub fn api(_: proc_macro::TokenStream) -> proc_macro::TokenStream { proc_macro::TokenStream::new() }',
            'attribute': '#[proc_macro_attribute] pub fn api(_: proc_macro::TokenStream, x: proc_macro::TokenStream) -> proc_macro::TokenStream { x }',
        }.items():
            with self.subTest(kind=kind):
                old = self.package(kind + '-old', 'extern crate proc_macro;', proc_macro=True)
                new = self.package(kind + '-new', 'extern crate proc_macro;' + source, proc_macro=True)
                self.assertEqual(self.compare(old, new)['impact'], 'minor')
                self.assertEqual(self.compare(new, old)['impact'], 'major')

    def test_macro_body_limit(self):
        prefix = 'extern crate proc_macro; #[proc_macro] pub fn api(x: proc_macro::TokenStream) -> proc_macro::TokenStream '
        old = self.package('old', prefix + '{ x }', proc_macro=True)
        new = self.package('new', prefix + '{ proc_macro::TokenStream::new() }', proc_macro=True)
        report = self.compare(old, new)
        self.assertEqual(report['impact'], 'patch')
        self.assertIn('macro expansion', report['limits'][0])

    def test_packaged_transitive_and_macro_dependencies(self):
        sides = []
        for side in ('old', 'new'):
            leaf = self.package(side + '-leaf', '#[cfg(feature="unwanted")] compile_error!("dependency defaults"); #[cfg(feature="needed")] pub struct Api;',
                                features='default=["unwanted"]\nunwanted=[]\nneeded=[]', name='fixture-leaf')
            middle = self.package(side + '-middle', 'pub use fixture_leaf::Api;', name='fixture-middle',
                                  dependencies='fixture-leaf={version="=1.0.0", default-features=false, features=["needed"]}')
            macro = self.package(side + '-macro', 'extern crate proc_macro; #[proc_macro] pub fn identity(x: proc_macro::TokenStream) -> proc_macro::TokenStream { x }', proc_macro=True, name='fixture-macro')
            root = self.package(side, 'pub use fixture_middle::Api; fixture_macro::identity! { pub fn api() {} }',
                                dependencies='fixture-middle="=1.0.0"\nfixture-macro="=1.0.0"')
            root['dependencies'] = [d['package'] for d in (leaf, middle, macro)]
            sides.append(root)
        self.assertEqual(self.compare(*sides)['impact'], 'patch')
        sides[1]['dependencies'] = sides[1]['dependencies'][1:]
        self.compare(*sides, expected='analysis-error')

    def test_missing_requested_feature(self):
        old = self.package('old', '')
        old['features'] = ['missing']
        report = self.compare(old, self.package('new', ''), expected='analysis-error')
        self.assertIn('old JSON production', report['diagnostics'])
        self.assertNotIn('impact', report)

    def test_compile_failure(self):
        report = self.compare(self.package('old', ''), self.package('new', 'compile_error!("fixture");'), expected='analysis-error')
        self.assertIn('new JSON production', report['diagnostics'])

    def test_wrong_producer(self):
        self.compare(self.package('old', ''), self.package('new', ''), expected='analysis-error', toolchain='nonexistent-357-fixture')

    def test_missing_explicit_default_policy(self):
        old = self.package('old', '')
        del old['default_features']
        self.compare(old, self.package('new', ''), expected='analysis-error')

    def test_source_dependency_path_rejected(self):
        old = self.package('old', '', dependencies='outside={path="../../outside"}')
        self.compare(old, self.package('new', ''), expected='analysis-error')

    def test_missing_analyzer(self):
        self.compare(self.package('old', ''), self.package('new', ''),
                     expected='analysis-error', analyzer=self.root / 'missing-tool')

    def test_wrong_analyzer_version(self):
        tool = self.root / 'wrong-tool'
        tool.write_text('#!/bin/sh\necho cargo-semver-checks 0.0.0\n')
        tool.chmod(0o755)
        self.compare(self.package('old', ''), self.package('new', ''),
                     expected='analysis-error', analyzer=tool)

    def test_wrapper_read_and_parse_errors(self):
        malformed = self.root / 'broken.json'
        malformed.write_text('{')
        for path in (malformed, self.root / 'absent.json'):
            with self.subTest(path=path.name):
                result = subprocess.run([os.environ['PACKAGE_API_DIFF_WRAPPER'], 'public-api-diff',
                                         '--old', str(path), '--new', str(path)], capture_output=True)
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse(result.stdout)


class JsonBoundary(unittest.TestCase):
    def test_schema_marker_alone_cannot_establish_compatibility(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'wrong-schema.json'
            path.write_text('{"format_version":999}')
            with self.assertRaisesRegex(AnalysisError, 'schema'):
                validate_json(path, 'probe', '1.0.0', 'x86_64-unknown-linux-gnu')

    def test_malformed_and_missing_json(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'bad.json'
            with self.assertRaises(OSError):
                validate_json(path, 'probe', '1.0.0', 'x86_64-unknown-linux-gnu')
            path.write_text('{')
            with self.assertRaises(ValueError):
                validate_json(path, 'probe', '1.0.0', 'x86_64-unknown-linux-gnu')


class SemverOutput(unittest.TestCase):
    def test_truncated_output(self):
        with self.assertRaises(AnalysisError):
            parse_semver('Checked [ 0.001s] 229 checks: 228 pass, 0 fail, 1 warn, 31 skip', 0)

    def test_failed_tool(self):
        with self.assertRaises(AnalysisError):
            parse_semver('', 101)

    def test_unknown_format(self):
        with self.assertRaises(AnalysisError):
            parse_semver('Checked all fine', 0)

    def test_inconsistent_status(self):
        output = 'Checked [ 0.001s] 229 checks: 229 pass, 31 skip\nSummary no semver update required'
        with self.assertRaises(AnalysisError):
            parse_semver(output, 100)


if __name__ == '__main__':
    unittest.main()
