"""Lightweight repository intent contracts: no compiler/analyzer rebuilds."""
import json
from pathlib import Path
import sys
import tempfile
import tarfile
import subprocess
import unittest
from unittest import mock

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / '.local/sbin/package-api'))
from compare import AnalysisError
from intent import intent_results, prepare_tools
from repository import release_intent, package_snapshot


class Intent(unittest.TestCase):
    def test_wrapper_uses_host_artifact_with_ambient_target_selection(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            executable = root / 'custom-target/host-triple/debug/xbuild'
            executable.parent.mkdir(parents=True)
            executable.touch()
            artifact = dict(reason='compiler-artifact', executable=str(executable),
                            target=dict(name='xbuild', kind=['bin']))
            replies = [subprocess.CompletedProcess([], 0, 'cargo-semver-checks 0.51.0-sqlfun.1\n'),
                       subprocess.CompletedProcess([], 0, 'rustc test\nhost: host-triple\n'),
                       subprocess.CompletedProcess([], 0, json.dumps(artifact) + '\n')]
            for ambient in ('host-triple', 'cross-triple'):
                with self.subTest(ambient=ambient), \
                     mock.patch('intent.repository_environment', return_value={'CARGO_BUILD_TARGET': ambient}), \
                     mock.patch('intent.shutil.which', return_value='/tools/cargo-semver-checks'), \
                     mock.patch('intent.run', side_effect=replies) as run:
                    wrapper, _ = prepare_tools('stable', root)
                    self.assertEqual(wrapper, executable)
                    command = run.call_args.args[0]
                    self.assertEqual(command[command.index('--target') + 1], 'host-triple')
                    self.assertIn('--message-format=json', command)
                    self.assertEqual(run.call_args.args[2]['CARGO_BUILD_TARGET'], ambient)

    def report(self, impact='patch'):
        return dict(comparisons=[dict(package='checked', configuration='postgres', kind='released',
                                     result=dict(impact=impact))], removed_coverage=[], new_coverage=[])

    def test_each_declared_level_is_an_upper_bound(self):
        levels = ['patch', 'minor', 'major']
        for declared in levels:
            for impact in levels:
                result = intent_results(self.report(impact), {'checked': declared})[0]
                self.assertEqual(result['outcome'] == 'pass', levels.index(impact) <= levels.index(declared))

    def test_evidence_is_reduced_per_crate_including_coverage(self):
        report = self.report()
        report['comparisons'].append(dict(package='other', configuration='sqlite', kind='released',
                                          result=dict(impact='major')))
        report['new_coverage'] = [dict(package='new', reason='new-package'),
                                 dict(package='checked', configuration='sqlite', reason='new-coverage', released_selection=True)]
        report['impact'] = 'major'
        results = intent_results(report, {'checked': 'patch', 'other': 'major'})
        self.assertEqual([r['impact'] for r in results], ['patch', 'major'])
        report['new_coverage'][-1]['released_selection'] = False
        self.assertEqual(intent_results(report, {'checked': 'minor', 'other': 'major'})[0]['impact'], 'minor')
        report['removed_coverage'] = [dict(package='checked', reason='released-manifest-features-removed')]
        self.assertEqual(intent_results(report, {'checked': 'major', 'other': 'major'})[0]['impact'], 'major')

    def test_intent_is_required_only_for_checked_crates(self):
        for value in (None, {}, {'next-release': None}, {'next-release': {}},
                      {'next-release': {'checked': 'invalid'}}, {'next-release': {'checked': 1}}):
            with self.subTest(value=value), self.assertRaises(AnalysisError):
                release_intent({'metadata': value}, {'checked'})
        self.assertEqual(release_intent({'metadata': {'next-release': {'checked': 'patch'}}}, {'checked'}), {'checked': 'patch'})

    def test_snapshot_retains_unpublished_packages_for_adoption(self):
        from lifecycle_helpers import write_package
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            packages = root / 'input'
            write_package(packages, name='checked')
            write_package(packages, name='new')
            target = root / 'target/package'
            target.mkdir(parents=True)
            for name in ('checked', 'new'):
                with tarfile.open(target / f'{name}-1.0.0.crate', 'w:gz') as archive:
                    archive.add(packages / name, arcname=f'{name}-1.0.0')
            manifest = 'checked\t1.0.0\t/checked/Cargo.toml\nnew\t1.0.0\t/new/Cargo.toml\n'
            replies = [subprocess.CompletedProcess([], 0, ''), subprocess.CompletedProcess([], 0, manifest)]
            provenance = dict(mode='commit', revision='a' * 40, dirty=False)
            with mock.patch('repository.run', side_effect=replies):
                descriptor, _ = package_snapshot(root, provenance, {'CARGO_TARGET_DIR': str(root / 'target')},
                    {'postgres': {'checked': [], 'new': []}}, 'target', root / 'candidate')
            state = json.loads(descriptor.read_text())
            self.assertEqual(set(state['packages']), {'checked', 'new'})
            self.assertEqual(set(state['configurations']['postgres']), {'checked', 'new'})
            self.assertTrue(all((descriptor.parent / p / 'Cargo.toml').exists() for p in state['packages'].values()))

    def test_failed_package_production_has_no_partial_output(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            output = root / 'candidate'
            with mock.patch('repository.run', side_effect=AnalysisError('packaging failed')):
                with self.assertRaisesRegex(AnalysisError, 'packaging failed'):
                    package_snapshot(root, {}, {}, {}, 'target', output)
            self.assertFalse(output.exists())


if __name__ == '__main__':
    unittest.main()
