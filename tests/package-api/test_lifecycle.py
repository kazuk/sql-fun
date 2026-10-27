"""Maintained lifecycle contracts, including the approved real initial baseline."""
import copy
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

sys.dont_write_bytecode = True
from lifecycle_helpers import ROOT, RealBaseline, SelectionChange, state_for, write_package
sys.path.insert(0, str(ROOT / '.local/sbin/package-api'))
from compare import AnalysisError
from lifecycle import read_state, schedule, execute, resolve_dependencies


class Scheduling(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix='coverage-contract-')
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.old = self.root / 'old'
        self.new = self.root / 'new'
        write_package(self.old, features='released=[]\n')
        write_package(self.new, features='released=[]\nadded=[]\n')
        self.before = state_for({'probe': 'probe'}, {'postgres': {'probe': ['released']}})
        self.after = copy.deepcopy(self.before)

    def inputs(self):
        result = []
        for root, state in ((self.old, self.before), (self.new, self.after)):
            path = root / 'coverage.json'
            path.write_text(json.dumps(state))
            result.extend(read_state(path))
        return result

    def test_growth_preserves_historical_replay_and_compares_expanded_selection(self):
        self.after['configurations']['postgres']['probe'].append('added')
        calls, removed, added, _ = schedule(*self.inputs())
        self.assertFalse(removed)
        self.assertEqual([c['kind'] for c in calls], ['released', 'expanded'])
        self.assertEqual(calls[0]['new']['features'], ['released'])
        self.assertEqual(calls[1]['old']['features'], ['released'])
        self.assertEqual(calls[1]['new']['features'], ['added', 'released'])

    def test_current_only_product_never_exempts_historical_comparison(self):
        self.after['configurations']['future-product'] = {'probe': ['added']}
        calls, removed, added, _ = schedule(*self.inputs())
        self.assertEqual([c['kind'] for c in calls], ['released', 'new-coverage'])
        self.assertFalse(removed)
        self.assertTrue(added)
        self.assertEqual(calls[1]['old'], calls[1]['new'])

    def test_new_name_for_released_selection_reuses_existing_comparison(self):
        self.after['configurations']['alias'] = {'probe': ['released']}
        calls, removed, added, _ = schedule(*self.inputs())
        self.assertEqual(len(calls), 1)
        self.assertFalse(removed)
        self.assertTrue(added[0]['released_selection'])

    def test_coverage_removal_is_not_growth(self):
        self.after['configurations']['postgres']['probe'] = []
        _, removed, added, _ = schedule(*self.inputs())
        self.assertEqual(removed[0]['reason'], 'released-configured-coverage-removed')
        self.assertFalse(added)

    def test_manifest_removal_does_not_request_missing_feature(self):
        (self.new / 'probe/Cargo.toml').write_text('[package]\nname="probe"\nversion="1.0.0"\n')
        self.after['configurations']['postgres']['probe'] = []
        calls, removed, _, _ = schedule(*self.inputs())
        self.assertEqual([c['kind'] for c in calls], ['current-validation'])
        self.assertEqual(calls[0]['new']['features'], [])
        self.assertIn('released-manifest-features-removed', [r['reason'] for r in removed])

    def test_new_crate_then_adoption_then_removal(self):
        write_package(self.new, name='new-crate')
        self.after['packages']['new-crate'] = 'new-crate'
        self.after['configurations']['postgres']['new-crate'] = []
        old, old_packages, new, new_packages = self.inputs()
        calls, removed, added, _ = schedule(old, old_packages, new, new_packages)
        self.assertFalse(removed)
        self.assertEqual([c['package'] for c in calls], ['probe'])
        self.assertEqual(added[0]['package'], 'new-crate')
        self.assertEqual(added[0]['reason'], 'new-package')
        calls, removed, added, _ = schedule(new, new_packages, new, new_packages)
        self.assertFalse(added)
        self.assertTrue(all(c['kind'] == 'released' for c in calls))
        _, removed, _, _ = schedule(new, new_packages, old, old_packages)
        self.assertIn(dict(package='new-crate', reason='released-package-removed'), removed)

    def test_removed_product_is_reported(self):
        self.after['configurations'] = {'replacement': {'probe': []}}
        _, removed, _, _ = schedule(*self.inputs())
        self.assertEqual(removed[0]['reason'], 'released-configured-coverage-removed')

    def test_invalid_mappings_and_unknown_features_are_errors(self):
        for mapping in ({}, {'private': []}, {'probe': ['missing']}, {'probe': 'released'}):
            with self.subTest(mapping=mapping):
                self.after['configurations']['postgres'] = mapping
                with self.assertRaises(AnalysisError):
                    self.inputs()

    def test_binary_packages_are_explicitly_excluded(self):
        for root, state in ((self.old, self.before), (self.new, self.after)):
            write_package(root, name='tool', source='fn main() {}', binary=True)
            state['packages']['tool'] = 'tool'
            state['configurations']['postgres']['tool'] = []
        _, _, added, excluded = schedule(*self.inputs())
        self.assertFalse(added)
        self.assertEqual(excluded[0]['reason'], 'binary-only')
        self.assertEqual(len(excluded), 1)

    def test_new_library_on_existing_binary_package_has_no_old_api_contract(self):
        (self.old / 'probe/src/lib.rs').unlink()
        (self.old / 'probe/src/main.rs').write_text('fn main() {}')
        calls, removed, added, _ = schedule(*self.inputs())
        self.assertFalse(removed)
        self.assertEqual(added[0]['reason'], 'new-api-target')
        self.assertEqual(calls[0]['old'], calls[0]['new'])

    def test_sides_resolve_their_own_packaged_dependency_closure(self):
        write_package(self.new, name='new-dependency')
        with (self.new / 'probe/Cargo.toml').open('a') as manifest:
            manifest.write('\n[dependencies]\nnew-dependency="1.0"\n')
        self.after['packages']['new-dependency'] = 'new-dependency'
        self.after['configurations']['postgres']['new-dependency'] = []
        calls, _, _, _ = schedule(*self.inputs())
        self.assertEqual(calls[0]['old']['dependencies'], [])
        self.assertEqual(calls[0]['new']['dependencies'], [str(self.new / 'new-dependency')])

    def test_full_seed_has_replayable_complete_configuration_mappings(self):
        import tarfile
        with tarfile.open(ROOT / '.local/semver_baseline.tar.gz') as archive:
            archive.extractall(self.root / 'seed', filter='data')
        state, packages = read_state(self.root / 'seed/baseline.json')
        calls, removed, added, excluded = schedule(state, packages, state, packages)
        self.assertFalse(removed)
        self.assertFalse(added)
        self.assertEqual({c['package'] for c in calls} | {e['package'] for e in excluded}, set(packages))

    def test_lost_released_coverage_remains_major_without_api_diff(self):
        self.inputs()
        policy = self.root / 'policy.toml'
        policy.write_text('[feature-set.postgres]\nprobe=["released"]\n')
        report = dict(status='detected', impact='patch', coverage={
            'old': {'resolved_features': ['released']}, 'new': {'resolved_features': []}})
        completed = subprocess.CompletedProcess([], 0, json.dumps(report))
        with mock.patch('lifecycle.subprocess.run', return_value=completed):
            result = execute(self.old / 'coverage.json', self.new / 'coverage.json', policy,
                             Path('/comparator'), Path('/wrapper'), Path('/semver'), 'stable', self.root)
        self.assertEqual(result['impact'], 'major')
        self.assertEqual(result['removed_coverage'][0]['features'], ['released'])

    def test_comparator_failure_has_no_success_verdict(self):
        self.inputs()
        policy = self.root / 'policy.toml'
        policy.write_text('[feature-set.postgres]\nprobe=["released"]\n')
        failed = subprocess.CompletedProcess([], 1, json.dumps({'status': 'analysis-error', 'diagnostics': 'build failed'}))
        with mock.patch('lifecycle.subprocess.run', return_value=failed), self.assertRaisesRegex(AnalysisError, 'build failed'):
            execute(self.old / 'coverage.json', self.new / 'coverage.json', policy,
                    Path('/comparator'), Path('/wrapper'), Path('/semver'), 'stable', self.root)


class RealLifecycle(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        prepared = subprocess.run([str(ROOT / '.local/sbin/prepare-package-api-tests')],
                                  text=True, capture_output=True, check=True)
        cls.tools = json.loads(prepared.stdout)
        RealBaseline.prepare_registry(cls.tools['toolchain'])

    def check(self, fixture, expected='detected'):
        fixture.write()
        completed = subprocess.run([sys.executable, str(ROOT / '.local/sbin/compare-feature-coverage'),
            '--baseline', str(fixture.baseline), '--candidate', str(fixture.descriptor),
            '--policy', str(fixture.policy), '--diff-wrapper', os.environ['PACKAGE_API_DIFF_WRAPPER'],
            '--semver-checks', self.tools['semver_checks'], '--toolchain', self.tools['toolchain']],
            text=True, capture_output=True)
        report = json.loads(completed.stdout)
        self.assertEqual(completed.returncode, 0 if expected == 'detected' else 1, report)
        self.assertEqual(report['status'], expected, report)
        if expected == 'detected':
            print(f'lifecycle: {report["impact"]}, {len(report["comparisons"])} comparisons', flush=True)
        else:
            self.assertNotIn('impact', report)
            self.assertNotIn('next_coverage', report)
        return report

    def test_replacement_and_shrink_validate_current_selection(self):
        for replacement, retain in ((True, True), (True, False), (False, True), (False, False)):
            with self.subTest(replacement=replacement, retain_released=retain):
                with tempfile.TemporaryDirectory(prefix='selected-coverage-') as directory:
                    fixture = SelectionChange(Path(directory), replacement, retain)
                    report = self.check(fixture)
                    self.assertEqual(report['impact'], 'major')
                    self.assertTrue(report['removed_coverage'])
                    self.assertTrue(any(c['kind'] == 'current-validation' for c in report['comparisons']))
                    fixture.invalidate_current_selection()
                    report = self.check(fixture, expected='analysis-error')
                    self.assertIn('invalid selected configuration', report['diagnostics'])

    def test_optional_local_crate_is_supplied_only_when_resolved(self):
        with tempfile.TemporaryDirectory(prefix='optional-local-crate-') as directory:
            root = Path(directory)
            write_package(root, name='probe')
            write_package(root, name='local-dependency')
            with (root / 'probe/Cargo.toml').open('a') as manifest:
                manifest.write('\n[dependencies]\nlocal-dependency={version="1.0",optional=true}\n')
            side = dict(package=str(root / 'probe'), dependencies=[str(root / 'local-dependency')],
                        default_features=True, features=[])
            selected = resolve_dependencies(side, 'x86_64-unknown-linux-gnu', self.tools['toolchain'])
            self.assertFalse(selected['dependencies'])
            side['features'] = ['local-dependency']
            selected = resolve_dependencies(side, 'x86_64-unknown-linux-gnu', self.tools['toolchain'])
            self.assertEqual(selected['dependencies'], side['dependencies'])

    def test_seven_acceptance_cases_from_real_seed(self):
        # One lifecycle sequence intentionally shares its expensive real-source
        # fixture; the adopted state is the output of the preceding stage.
        with tempfile.TemporaryDirectory(prefix='real-seed-lifecycle-') as directory:
            fixture = RealBaseline(Path(directory))
            self.assertEqual(fixture.state['source_revision'], 'bd53b083b028dc8423f6c4e5b232cbd3c851c8c1')
            name = write_package(fixture.candidate, name='new-crate')
            fixture.current['packages'][name] = name
            fixture.current['configurations']['postgres'][name] = []
            report = self.check(fixture)
            self.assertEqual(report['impact'], 'patch')
            self.assertFalse(any(c['package'] == name for c in report['comparisons']))
            self.assertEqual(report['next_coverage']['packages'][name], name)
            fixture.add_feature()
            report = self.check(fixture)  # AC 1, 7: additive unreleased coverage.
            self.assertEqual(report['impact'], 'minor')
            self.assertFalse(report['removed_coverage'])
            fixture.break_released_api()
            self.assertEqual(self.check(fixture)['impact'], 'major')  # AC 2.
            fixture.add_feature()  # Restore original API and keep the new feature.
            fixture.current['configurations']['new-product'] = {fixture.name: ['unreleased'], name: []}
            self.assertEqual(self.check(fixture)['impact'], 'minor')  # AC 3.
            fixture.break_released_api()
            report = self.check(fixture)  # AC 4, also a new crate cannot hide the break.
            self.assertEqual(report['impact'], 'major')
            self.assertTrue(any(c['package'] == name for c in report['new_coverage']))
            self.assertFalse(any(c['package'] == name for c in report['comparisons']))
            fixture.add_feature()
            fixture.adopt()
            fixture.break_added_api()
            self.assertEqual(self.check(fixture)['impact'], 'major')  # AC 5.
            (fixture.candidate / 'new-crate/src/lib.rs').write_text('pub fn released(_: u8) {}')
            report = self.check(fixture)
            self.assertTrue(any(c['package'] == 'new-crate' and c['result']['impact'] == 'major'
                                for c in report['comparisons']))
            fixture.manifest.write_text(fixture.original_manifest)
            fixture.lib.write_text(fixture.original_api)
            (fixture.candidate / 'new-crate/src/lib.rs').write_text('pub fn released() {}')
            for mapping in fixture.current['configurations'].values():
                mapping[fixture.name] = []
            report = self.check(fixture)  # AC 6, 7: released coverage removal.
            self.assertEqual(report['impact'], 'major')
            self.assertTrue(report['removed_coverage'])


if __name__ == '__main__':
    unittest.main()
