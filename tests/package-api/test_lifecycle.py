"""Lightweight orchestration contracts; compiler behavior belongs to test_compare."""
import copy
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

sys.dont_write_bytecode = True
from lifecycle_helpers import ROOT, state_for, write_package, write_resource_baseline
sys.path.insert(0, str(ROOT / '.local/sbin/package-api'))
from compare import AnalysisError
from lifecycle import read_state, schedule, execute


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
            if root == self.old:
                write_resource_baseline(root, state)
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
        self.assertIsNone(calls[1]['old'])

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

    def test_new_crate_then_adoption_then_exclusion(self):
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
        _, removed, _, excluded = schedule(new, new_packages, old, old_packages)
        self.assertFalse(any(item['package'] == 'new-crate' for item in removed))
        self.assertIn(dict(package='new-crate', reason='no-longer-publishable'), excluded)

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
        self.assertIsNone(calls[0]['old'])

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
        self.assertEqual(state['schema'], 2)
        self.assertFalse((self.root / 'seed/packages').exists())
        calls, removed, added, excluded = schedule(state, packages, state, packages)
        self.assertFalse(removed)
        self.assertFalse(added)
        self.assertEqual({c['package'] for c in calls} | {e['package'] for e in excluded}, set(packages))

    def execute_with_reports(self, reports):
        self.inputs()
        policy = self.root / 'policy.toml'
        policy.write_text(''.join(
            f'[feature-set.{product}]\n' + ''.join(
                f'{name}={json.dumps(features)}\n' for name, features in mapping.items())
            for product, mapping in self.after['configurations'].items()))
        completed = [subprocess.CompletedProcess([], 0 if report['status'] == 'detected' else 1,
                                                json.dumps(report)) for report in reports]
        with mock.patch('lifecycle.subprocess.run', side_effect=completed) as comparator:
            result = execute(self.old / 'baseline.json', self.new / 'coverage.json', policy,
                             Path('/comparator'), Path('/wrapper'), Path('/semver'), 'stable', self.root)
        self.assertEqual(comparator.call_count, len(reports))
        return result

    @staticmethod
    def api_report(impact='patch'):
        return dict(status='detected', impact=impact, coverage={
            'old': {'resolved_features': ['released']},
            'new': {'resolved_features': ['released']}})

    def test_replacement_and_shrink_propagate_current_selection_failure(self):
        for replacement, retain in ((True, True), (True, False), (False, True), (False, False)):
            with self.subTest(replacement=replacement, retain_released=retain):
                features = ('released=[]\n' if retain else '') + 'replacement=[]\n'
                (self.new / 'probe/Cargo.toml').write_text(
                    '[package]\nname="probe"\nversion="1.0.0"\n[features]\n' + features)
                selected = ['replacement'] if replacement else []
                self.after['configurations']['postgres']['probe'] = selected
                calls, _, _, _ = schedule(*self.inputs())
                self.assertEqual(calls[-1]['kind'], 'current-validation')
                self.assertEqual(calls[-1]['new']['features'], selected)
                valid = [self.api_report() for _ in calls]
                result = self.execute_with_reports(valid)
                self.assertEqual(result['impact'], 'major')
                self.assertTrue(result['removed_coverage'])
                failed = valid[:-1] + [dict(status='analysis-error', diagnostics='build failed')]
                with self.assertRaisesRegex(AnalysisError, 'build failed'):
                    self.execute_with_reports(failed)

    def test_feature_growth_aggregates_comparator_impact(self):
        self.after['configurations']['postgres']['probe'].append('added')
        for impact in ('patch', 'minor', 'major'):
            with self.subTest(comparator_impact=impact):
                result = self.execute_with_reports([self.api_report(impact), self.api_report('minor')])
                self.assertEqual(result['impact'], 'major' if impact == 'major' else 'minor')
                self.assertEqual(result['next_coverage'], self.after)

    def test_new_crate_is_neutral_until_adopted(self):
        write_package(self.new, name='new-crate')
        self.after['packages']['new-crate'] = 'new-crate'
        self.after['configurations']['postgres']['new-crate'] = []
        result = self.execute_with_reports([self.api_report()])
        self.assertEqual(result['impact'], 'patch')
        self.assertEqual([c['package'] for c in result['comparisons']], ['probe'])
        self.assertIn('new-crate', result['next_coverage']['packages'])
        # Once adopted, the new crate is a released comparison obligation.
        write_package(self.old, name='new-crate')
        self.before = copy.deepcopy(result['next_coverage'])
        result = self.execute_with_reports([self.api_report(), self.api_report('major')])
        self.assertEqual(result['impact'], 'major')
        self.assertTrue(any(c['package'] == 'new-crate' and c['result']['impact'] == 'major'
                            for c in result['comparisons']))

    def test_lost_released_coverage_remains_major_without_api_diff(self):
        self.inputs()
        policy = self.root / 'policy.toml'
        policy.write_text('[feature-set.postgres]\nprobe=["released"]\n')
        report = dict(status='detected', impact='patch', coverage={
            'old': {'resolved_features': ['released']}, 'new': {'resolved_features': []}})
        completed = subprocess.CompletedProcess([], 0, json.dumps(report))
        with mock.patch('lifecycle.subprocess.run', return_value=completed):
            result = execute(self.old / 'baseline.json', self.new / 'coverage.json', policy,
                             Path('/comparator'), Path('/wrapper'), Path('/semver'), 'stable', self.root)
        self.assertEqual(result['impact'], 'major')
        self.assertEqual(result['removed_coverage'][0]['features'], ['released'])

    def test_comparator_failure_has_no_success_verdict(self):
        self.inputs()
        policy = self.root / 'policy.toml'
        policy.write_text('[feature-set.postgres]\nprobe=["released"]\n')
        failed = subprocess.CompletedProcess([], 1, json.dumps({'status': 'analysis-error', 'diagnostics': 'build failed'}))
        with mock.patch('lifecycle.subprocess.run', return_value=failed), self.assertRaisesRegex(AnalysisError, 'build failed'):
            execute(self.old / 'baseline.json', self.new / 'coverage.json', policy,
                    Path('/comparator'), Path('/wrapper'), Path('/semver'), 'stable', self.root)


if __name__ == '__main__':
    unittest.main()
