"""Archive/adoption contracts maintained by version-materialization, without builds."""
import copy
import io
import json
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import unittest
from unittest import mock

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / '.local/sbin/package-api'))
from baseline_archive import adopt, write_candidate
from compare import AnalysisError, compare
from lifecycle import execute, unpack_baseline
from resources import produce_resources, read_baseline, retain_resource
from lifecycle_helpers import state_for, write_package, write_resource_baseline


class Baselines(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.source = self.root / 'source'
        self.source.mkdir()
        write_package(self.source, features='extra=[]\n')
        write_package(self.source, name='new')
        write_package(self.source, name='binary', binary=True)
        self.state = state_for({n: n for n in ('probe', 'new', 'binary')})
        self.state['source_revision'] = 'a' * 40
        self.state['source'] = dict(mode='commit', revision='a' * 40, dirty=False)
        self.descriptor = self.source / 'coverage.json'
        self.descriptor.write_text(json.dumps(self.state))
        self.baseline = write_resource_baseline(self.source, self.state)
        self.resources = json.loads(self.baseline.read_text())

    def bundle(self, label, state=None):
        directory = self.root / label
        directory.mkdir()
        state = copy.deepcopy(state or self.resources)
        for record in state['packages'].values():
            for resource in record['selections']:
                (directory / resource['rustdoc']).write_bytes((self.source / resource['rustdoc']).read_bytes())
        descriptor = directory / 'baseline.json'
        descriptor.write_text(json.dumps(state))
        archive = self.root / f'{label}.tar.gz'
        write_candidate(descriptor, archive)
        return descriptor, archive

    def test_deterministic_archive_and_stable_retry_snapshot_noop(self):
        descriptor, active = self.bundle('active')
        _, candidate = self.bundle('candidate')
        self.assertEqual(active.read_bytes(), candidate.read_bytes())
        for kind in ('snapshot', 'prerelease'):
            before = (active.read_bytes(), active.stat().st_mtime_ns)
            result = adopt(self.root / 'absent', active, kind)
            self.assertEqual(result['status'], 'unchanged')
            self.assertEqual(before, (active.read_bytes(), active.stat().st_mtime_ns))
        for _ in range(2):
            self.assertEqual(adopt(candidate, active, 'stable')['status'], 'adopted')
            self.assertEqual(active.read_bytes(), candidate.read_bytes())
        with self.assertRaises(AnalysisError):
            write_candidate(descriptor, candidate)
        with self.assertRaises(AnalysisError):
            adopt(candidate, self.root / 'missing', 'stable')

    def test_invalid_candidate_and_active_preserve_state(self):
        _, active = self.bundle('active')
        _, candidate = self.bundle('candidate')
        good = candidate.read_bytes()
        for invalid in (b'not an archive', good[:40]):
            candidate.write_bytes(invalid)
            before = active.read_bytes()
            with self.assertRaises((AnalysisError, tarfile.TarError, EOFError)):
                adopt(candidate, active, 'stable')
            self.assertEqual(active.read_bytes(), before)
        candidate.write_bytes(good)
        active.write_bytes(b'corrupt active baseline')
        with self.assertRaises(tarfile.TarError):
            adopt(candidate, active, 'stable')
        self.assertEqual(active.read_bytes(), b'corrupt active baseline')

    def test_archive_paths_duplicates_links_and_missing_descriptor_rejected(self):
        for name, kind in (('../escape', 'file'), ('/absolute', 'file'), ('duplicate', 'duplicate'),
                           ('link', 'symlink'), ('device', 'device'), ('unrelated', 'file')):
            with self.subTest(name=name):
                archive = self.root / 'invalid.tar.gz'
                with tarfile.open(archive, 'w:gz') as output:
                    member = tarfile.TarInfo(name)
                    if kind == 'symlink':
                        member.type = tarfile.SYMTYPE
                        member.linkname = 'baseline.json'
                    if kind == 'device':
                        member.type = tarfile.CHRTYPE
                    output.addfile(member, io.BytesIO())
                    if kind == 'duplicate':
                        output.addfile(member, io.BytesIO())
                with tempfile.TemporaryDirectory() as destination:
                    with self.assertRaises((AnalysisError, OSError)):
                        unpack_baseline(archive, Path(destination))

    def test_resource_identity_digest_mapping_and_schema_validation(self):
        descriptor, _ = self.bundle('validation')
        for mutation in ('digest', 'identity', 'mapping', 'schema', 'absolute', 'producer', 'features'):
            state = copy.deepcopy(self.resources)
            resource = state['packages']['probe']['selections'][0]
            if mutation == 'digest':
                resource['sha256'] = '0' * 64
            elif mutation == 'identity':
                resource['coverage']['version'] = '9.0.0'
            elif mutation == 'mapping':
                del state['configurations']['postgres']['probe']
            elif mutation == 'schema':
                state['schema'] = 1
            elif mutation == 'producer':
                del resource['producer']['rustdoc']
            elif mutation == 'absolute':
                resource['rustdoc'] = str(self.source / resource['rustdoc'])
            else:
                resource['coverage']['features'] = 'extra'
            descriptor.write_text(json.dumps(state))
            with self.subTest(mutation=mutation), self.assertRaises(AnalysisError):
                read_baseline(descriptor)

    def test_retained_json_reused_and_missing_new_package_generated_once(self):
        retained = self.root / 'retained'
        probe = self.resources['packages']['probe']['selections'][0]
        retain_resource(retained / '0', self.source / probe['rustdoc'], probe['coverage'], probe['producer'])
        generated = []

        def generate(side, paths, scratch, target, host, env, context):
            # Exercise current_api's scratch contract; no real compiler is needed.
            scratch.mkdir()
            name = Path(side['package']).name
            generated.append(name)
            resource = self.resources['packages'][name]['selections'][0]
            artifact = scratch / 'api.json'
            artifact.write_bytes((self.source / resource['rustdoc']).read_bytes())
            return artifact, resource['coverage']

        with mock.patch('compare.generate', side_effect=generate), \
             mock.patch('compare.producer_environment', return_value=({}, probe['producer'])):
            result = produce_resources(self.descriptor, self.root / 'complete', 'stable', self.root, retained)
        self.assertEqual(generated, ['new'])
        state, _ = read_baseline(result)
        self.assertEqual(set(state['packages']), {'probe', 'new', 'binary'})
        self.assertFalse(state['packages']['binary']['selections'])
        resource = state['packages']['probe']['selections'][0]
        self.assertEqual((result.parent / resource['rustdoc']).read_bytes(), (self.source / probe['rustdoc']).read_bytes())

    def test_missing_selection_failure_removes_partial_bundle(self):
        output = self.root / 'incomplete'
        with mock.patch('resources.current_api', side_effect=AnalysisError('new current build failed')):
            with self.assertRaisesRegex(AnalysisError, 'new current build failed'):
                produce_resources(self.descriptor, output, 'stable', self.root)
        self.assertFalse(output.exists())

    def test_atomic_adoption_failure_preserves_previous_bytes(self):
        _, active = self.bundle('active')
        _, candidate = self.bundle('candidate')
        previous = active.read_bytes()
        with mock.patch('baseline_archive.os.replace', side_effect=OSError('replacement failed')):
            with self.assertRaisesRegex(OSError, 'replacement failed'):
                adopt(candidate, active, 'stable')
        self.assertEqual(active.read_bytes(), previous)
        self.assertFalse(list(active.parent.glob('.semver-*')))

    def test_comparator_retains_before_its_scratch_is_removed(self):
        resource = self.resources['packages']['probe']['selections'][0]
        output = self.root / 'retained'
        request = dict(old=None, new={}, target=self.state['target'], toolchain='stable',
                       cargo_context=str(self.root), resource_output=str(output))
        with tempfile.TemporaryDirectory() as scratch:
            artifact = Path(scratch) / 'current.json'
            original = (self.source / resource['rustdoc']).read_bytes()
            artifact.write_bytes(original)
            with mock.patch('compare.current_api', return_value=(artifact, resource['coverage'], resource['producer'], {})), \
                 mock.patch('compare.run', return_value=subprocess.CompletedProcess([], 0, 'cargo-semver-checks 0.51.0-sqlfun.1')), \
                 mock.patch('compare.analyze', return_value=(dict(added=[], removed=[], changed=[]), dict(minor=[], major=[]))):
                compare(request, Path('/wrapper'), Path('/semver'), Path(scratch))
        self.assertEqual((output / 'api.json').read_bytes(), original)

    def test_proc_macro_json_uses_producer_host(self):
        descriptor, _ = self.bundle('proc-macro')
        state = copy.deepcopy(self.resources)
        resource = state['packages']['probe']['selections'][0]
        resource['coverage']['target_kind'] = ['proc-macro']
        resource['coverage']['documented_target'] = 'host-triple'
        resource['producer']['host'] = 'host-triple'
        artifact = descriptor.parent / resource['rustdoc']
        document = json.loads(artifact.read_text())
        document['target']['triple'] = 'host-triple'
        artifact.write_text(json.dumps(document))
        import hashlib
        resource['sha256'] = hashlib.sha256(artifact.read_bytes()).hexdigest()
        descriptor.write_text(json.dumps(state))
        read_baseline(descriptor)
        resource['producer']['host'] = 'different-host'
        descriptor.write_text(json.dumps(state))
        with self.assertRaisesRegex(AnalysisError, 'documented target mismatch'):
            read_baseline(descriptor)

    def test_invocation_reuses_same_current_selection_across_comparisons(self):
        side = dict(package=str(self.source / 'probe'), dependencies=[], features=[], default_features=True)
        calls = [dict(package='probe', configuration='postgres', kind='released', old=None, new=side),
                 dict(package='probe', configuration='sqlite', kind='expanded', old=None, new=side)]
        resource = self.resources['packages']['probe']['selections'][0]
        requested = []

        def comparison(call, *args, resource_output=None):
            requested.append(call['new'])
            retain_resource(resource_output, self.source / resource['rustdoc'], resource['coverage'], resource['producer'])
            return dict(impact='patch', coverage=dict(old=resource['coverage'], new=resource['coverage']))

        with mock.patch('lifecycle.schedule', return_value=(calls, [], [], [])), \
             mock.patch('lifecycle.compare_selection', side_effect=comparison):
            execute(self.baseline, self.descriptor, None, None, None, None, 'stable', self.root,
                    resource_directory=self.root / 'retained')
        self.assertNotIn('rustdoc', requested[0])
        self.assertIn('rustdoc', requested[1])

    def test_candidate_failure_policy_and_single_package_snapshot(self):
        from argparse import Namespace
        from intent import validate
        args = Namespace(repository=str(self.root), revision='a' * 40, toolchain='stable',
                         target=self.state['target'], candidate_output=self.root / 'output.tar.gz')
        # Output lives outside the explicitly selected source repository.
        args.repository = str(self.source)
        provenance = self.state['source']
        report = dict(comparisons=[dict(package='probe', configuration='postgres', kind='released',
                                       result=dict(impact='major'))], removed_coverage=[], new_coverage=[], next_coverage={})
        prepared = (self.source, provenance, {}, self.baseline, {'probe': 'patch'}, self.state['configurations'])
        with mock.patch('intent.prepare_input', return_value=prepared), \
             mock.patch('intent.prepare_tools', return_value=(Path('/wrapper'), Path('/semver'))), \
             mock.patch('intent.package_snapshot', return_value=(self.descriptor, 0)) as package, \
             mock.patch('intent.execute', return_value=report), \
             mock.patch('intent.produce_resources') as produce:
            result = validate(args, self.root)
            self.assertEqual(result['status'], 'exceeds-intent')
            package.assert_called_once()
            produce.assert_not_called()
            self.assertFalse(args.candidate_output.exists())
        args.revision = None
        with mock.patch('intent.prepare_input') as prepare:
            with self.assertRaisesRegex(AnalysisError, 'commit mode'):
                validate(args, self.root)
            prepare.assert_not_called()

    def test_candidate_adoption_makes_new_features_historical(self):
        from lifecycle import read_state, schedule
        old, old_packages = read_baseline(self.baseline)
        self.state['configurations']['postgres']['probe'] = ['extra']
        self.descriptor.write_text(json.dumps(self.state))
        new, packages = read_state(self.descriptor)
        calls, _, _, _ = schedule(old, old_packages, new, packages)
        self.assertTrue(any(c['kind'] == 'expanded' for c in calls))
        # Supplied current JSON fixture represents the expanded surface.
        expanded = copy.deepcopy(self.resources)
        expanded['configurations']['postgres']['probe'] = ['extra']
        expanded['packages']['probe']['selections'][0]['coverage']['features'] = ['extra']
        expanded['packages']['probe']['selections'][0]['coverage']['resolved_features'] = ['extra']
        _, active = self.bundle('old')
        _, candidate = self.bundle('expanded', expanded)
        adopt(candidate, active, 'snapshot')
        self.assertEqual(active.read_bytes(), (self.root / 'old.tar.gz').read_bytes())
        adopt(candidate, active, 'stable')
        published = unpack_baseline(active, self.root / 'published')
        released, released_packages = read_baseline(published)
        self.state['configurations']['postgres']['probe'] = []
        self.descriptor.write_text(json.dumps(self.state))
        new, packages = read_state(self.descriptor)
        _, removed, _, _ = schedule(released, released_packages, new, packages)
        self.assertTrue(any(r['reason'] == 'released-configured-coverage-removed' for r in removed))


if __name__ == '__main__':
    unittest.main()
