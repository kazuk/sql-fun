"""Fast package-input and report contracts; invoked by xbuild's integration test."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
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

    def test_missing_requested_feature(self):
        old = self.package('old', '')
        old['features'] = ['missing']
        report = self.compare(old, self.package('new', ''), expected='analysis-error')
        self.assertIn('old JSON production', report['diagnostics'])
        self.assertNotIn('impact', report)

    def test_wrong_producer(self):
        self.compare(self.package('old', ''), self.package('new', ''), expected='analysis-error', toolchain='nonexistent-357-fixture')

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

    def test_preparation_rejects_stock_analyzer_without_building(self):
        tools = self.root / 'stock-tools'
        tools.mkdir()
        analyzer = tools / 'cargo-semver-checks'
        analyzer.write_text('#!/bin/sh\necho cargo-semver-checks 0.51.0\n')
        analyzer.chmod(0o755)
        cargo = tools / 'cargo'
        cargo.write_text('#!/bin/sh\necho unexpected-install >&2\nexit 99\n')
        cargo.chmod(0o755)
        result = subprocess.run([str(ROOT / '.local/sbin/prepare-package-api-tests')],
                                cwd=ROOT, env=dict(os.environ, PATH=str(tools) + os.pathsep + os.environ['PATH']),
                                text=True, capture_output=True)
        self.assertEqual(result.returncode, 1)
        self.assertIn('builder must provide cargo-semver-checks 0.51.0-sqlfun.1', result.stderr)
        self.assertNotIn('unexpected-install', result.stderr)
        self.assertFalse(result.stdout)

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
