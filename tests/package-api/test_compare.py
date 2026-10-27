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
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
sys.dont_write_bytecode = True
sys.path.insert(0, str(ROOT / '.local/sbin/package-api'))
from compare import AnalysisError, parse_semver, validate_json
import compare as comparator


def fingerprint(directory):
    return {str(p.relative_to(directory)): hashlib.sha256(p.read_bytes()).hexdigest()
            for p in directory.rglob('*') if p.is_file()}


class Comparison(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        prepared = subprocess.run([str(ROOT / '.local/sbin/prepare-package-api-tests')],
                                  text=True, capture_output=True, check=True)
        cls.tools = json.loads(prepared.stdout)
        details = subprocess.run(['rustup', 'run', cls.tools['toolchain'], 'rustc', '--version', '--verbose'],
                                 text=True, capture_output=True, check=True).stdout
        cls.tools['target'] = next(line.removeprefix('host: ') for line in details.splitlines() if line.startswith('host: '))

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
        request = dict(old=old, new=new, target=self.tools['target'], toolchain=self.tools['toolchain'])
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

    def test_unchanged_shadowed_trait_and_struct(self):
        # Keeping the private tuple type in a public signature makes rustdoc
        # retain it, as in the approved historical SQLAST package (#826).
        for definition in ('pub struct Api;', 'pub struct Api(Option<u8>);'):
            with self.subTest(definition=definition):
                source = ('mod traits { pub trait Api {} } '
                          'mod generated { mod inner { ' + definition + ' } '
                          'pub use self::inner::Api; pub fn keep(_: Api) {} } '
                          'pub mod api { pub use crate::traits::Api; pub use crate::generated::*; }')
                old = self.package('old-' + str(len(definition)), source)
                new = self.package('new-' + str(len(definition)), source)
                report = self.compare(old, new)
                self.assertEqual(report['impact'], 'patch')
                self.assertEqual(report['semver']['major'], 0)
                self.assertEqual(report['diff']['added'], [])
                self.assertEqual(report['diff']['removed'], [])
                self.assertEqual(report['diff']['changed'], [])

    def test_trait_replaced_by_another_kind(self):
        for kind, replacement in dict(struct='pub struct Api;', enum='pub enum Api {}',
                                      union='pub union Api { pub value: u8 }').items():
            with self.subTest(replacement=replacement):
                old = self.package('old-' + kind, 'pub trait Api {}')
                new = self.package('new-' + kind, replacement)
                report = self.compare(old, new)
                self.assertEqual(report['impact'], 'major')
                self.assertIn('failure trait_changed_kind:', report['semver']['diagnostics'])

    def test_trait_replacement_is_path_specific(self):
        shared = 'mod traits { pub trait Api {} } pub use traits::Api as Other; '
        report = self.compare(self.package('old', shared + 'pub use traits::Api;'),
                              self.package('new', shared + 'pub struct Api;'))
        self.assertEqual(report['impact'], 'major')
        self.assertIn('failure trait_changed_kind:', report['semver']['diagnostics'])

    def test_complete_approved_baseline_self_comparison(self):
        import baseline_helpers
        compared, excluded = [], []
        for name, selection in baseline_helpers.released_selections(
                ROOT, self.root, self.tools['toolchain'], self.tools['target']):
            with self.subTest(package=name):
                print(f'Approved baseline: {name}', file=sys.stderr, flush=True)
                if selection is None:
                    excluded.append(name)
                    continue
                report = self.compare(selection, selection)
                self.assertEqual(report['impact'], 'patch', report)
                compared.append(name)
        self.assertEqual(len(compared), 9)
        self.assertIn('sql-fun-sqlast', compared)
        self.assertEqual(excluded, ['adventure-works'])

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

    def request_for(self, old, new):
        return dict(old=old, new=new, target=self.tools['target'], toolchain=self.tools['toolchain'])

    def test_updated_compiler_versions_with_compatible_json(self):
        request = self.request_for(self.package('old', ''), self.package('new', 'pub fn added() {}'))
        scratch = self.root / 'analysis'
        scratch.mkdir()
        # Simulate a routine compiler update retaining a compatible schema.
        # JSON generation and both analyzers still execute their real binaries.
        real_run = comparator.run

        def updated_version(command, *args, **kwargs):
            result = real_run(command, *args, **kwargs)
            if command[1:] in (['--version'], ['--version', '--verbose']):
                name = Path(command[0]).name
                if name in ('rustc', 'cargo', 'rustdoc'):
                    lines = result.stdout.splitlines()
                    lines[0] = f'{name} 99.0.0 (updated-compiler-fixture 2099-01-01)'
                    result.stdout = '\n'.join(lines) + '\n'
            return result

        # An ambient standalone Cargo must not override the selected toolchain.
        tools = self.root / 'ambient-tools'
        tools.mkdir()
        cargo = tools / 'cargo'
        cargo.write_text('#!/bin/sh\necho wrong-ambient-cargo >&2\nexit 90\n')
        cargo.chmod(0o755)
        with mock.patch('compare.run', side_effect=updated_version), mock.patch.dict(
                os.environ, {'PATH': str(tools) + os.pathsep + os.environ['PATH']}):
            report = comparator.compare(request, Path(os.environ['PACKAGE_API_DIFF_WRAPPER']),
                                        Path(self.tools['semver_checks']), scratch)
        self.assertEqual(report['impact'], 'minor')
        for name in ('rustc', 'cargo', 'rustdoc'):
            self.assertIn('99.0.0', report['producer'][name])
        self.assertEqual(report['coverage']['old']['schema'], report['coverage']['new']['schema'])

    def test_unsupported_generated_schema_is_not_a_verdict(self):
        request = self.request_for(self.package('old', ''), self.package('new', ''))
        scratch = self.root / 'analysis'
        scratch.mkdir()
        real_generate = comparator.generate

        def unsupported_schema(*args):
            artifact, coverage = real_generate(*args)
            document = json.loads(artifact.read_text())
            document['format_version'] = 999
            artifact.write_text(json.dumps(document))
            coverage['schema'] = 999
            return artifact, coverage

        with mock.patch('compare.generate', side_effect=unsupported_schema):
            with self.assertRaises(AnalysisError) as failure:
                comparator.compare(request, Path(os.environ['PACKAGE_API_DIFF_WRAPPER']),
                                   Path(self.tools['semver_checks']), scratch)
        message = str(failure.exception)
        self.assertIn('schema 999', message)
        self.assertIn('update and re-probe the pinned analyzers', message)
        self.assertIn('Actual producer:', message)

    def test_preparation_uses_active_toolchain_without_installing(self):
        tools = self.root / 'tools'
        tools.mkdir()
        log = self.root / 'rustup-calls'
        rustup = tools / 'rustup'
        rustup.write_text('#!/bin/sh\nprintf "%s\\n" "$*" >> ' + str(log) + '\n'
                          'test "$*" = "show active-toolchain" || exit 99\n'
                          'echo "next-stable-fixture (environment override)"\n')
        analyzer = tools / 'cargo-semver-checks'
        analyzer.write_text('#!/bin/sh\necho cargo-semver-checks 0.51.0-sqlfun.1\n')
        rustup.chmod(0o755)
        analyzer.chmod(0o755)
        env = dict(os.environ, PATH=str(tools) + os.pathsep + os.environ['PATH'])
        prepared = subprocess.run([str(ROOT / '.local/sbin/prepare-package-api-tests')],
                                  cwd=ROOT, env=env, text=True, capture_output=True, check=True)
        self.assertEqual(json.loads(prepared.stdout)['toolchain'], 'next-stable-fixture')
        self.assertEqual(log.read_text().splitlines(), ['show active-toolchain'])

    def test_missing_explicit_default_policy(self):
        old = self.package('old', '')
        del old['default_features']
        self.compare(old, self.package('new', ''), expected='analysis-error')

    def test_source_dependency_path_rejected(self):
        old = self.package('old', '', dependencies='outside={path="../../outside"}')
        self.compare(old, self.package('new', ''), expected='analysis-error')

    def test_source_target_path_rejected(self):
        old = self.package('old', '')
        manifest = Path(old['package']) / 'Cargo.toml'
        outside = self.root / 'outside.rs'
        outside.write_text('pub fn outside() {}')
        manifest.write_text(manifest.read_text().replace('[lib]', '[lib]\npath=' + json.dumps(str(outside))))
        report = self.compare(old, self.package('new', ''), expected='analysis-error')
        self.assertIn('target source is outside', report['diagnostics'])

    def test_external_workspace_reference_rejected(self):
        old = self.package('old', '')
        manifest = Path(old['package']) / 'Cargo.toml'
        manifest.write_text(manifest.read_text().replace('[package]', '[package]\nworkspace="../other"'))
        report = self.compare(old, self.package('new', ''), expected='analysis-error')
        self.assertIn('must not reference an external workspace', report['diagnostics'])

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
    def test_schema_version_must_be_an_integer(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'wrong-schema.json'
            path.write_text('{"format_version":"61"}')
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
