"""Domain fixtures for package-set and released-coverage transitions."""
import copy
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import tarfile

ROOT = Path(__file__).resolve().parents[2]


def archive_state(directory, state, output):
    (directory / 'baseline.json').write_text(json.dumps(state))
    with tarfile.open(output, 'w:gz') as archive:
        archive.add(directory, arcname='.')
    return output


def state_for(packages, configurations=None):
    return dict(schema=1, source_revision='explicit-test-revision', target='x86_64-unknown-linux-gnu',
                default_features=True, packages=packages,
                configurations=configurations or {'postgres': {name: [] for name in packages}})


def write_package(root, name='probe', features='', source='pub fn released() {}', binary=False):
    package = root / name
    (package / 'src').mkdir(parents=True)
    (package / 'Cargo.toml').write_text(
        f'[package]\nname="{name}"\nversion="1.0.0"\nedition="2021"\n[features]\n{features}')
    (package / ('src/main.rs' if binary else 'src/lib.rs')).write_text(source)
    return name


class RealBaseline:
    """Extract the real seed and project core coverage for actual compiler tests.

    Heavy: callers run repeated isolated rustdoc/analyzer builds. No process cache
    is assumed across nextest tests. Unrelated released packages stay in the
    checked-in seed; this fixture scopes the acceptance mutations to core.
    """
    name = 'sql-fun-core'

    @staticmethod
    def prepare_registry(toolchain):
        # Preparation may download locked historical registry dependencies;
        # the production comparison itself always remains offline.
        with tempfile.TemporaryDirectory(prefix='seed-registry-') as directory:
            root = Path(directory)
            with tarfile.open(ROOT / '.local/semver_baseline.tar.gz') as archive:
                archive.extractall(root, filter='data')
            state = json.loads((root / 'baseline.json').read_text())
            subprocess.run(['rustup', 'run', toolchain, 'cargo', 'fetch', '--locked',
                            '--manifest-path', str(root / state['packages']['sql-fun-core'] / 'Cargo.toml')],
                           check=True, capture_output=True)

    def __init__(self, root):
        self.root = root
        self.released = root / 'released'
        with tarfile.open(ROOT / '.local/semver_baseline.tar.gz') as archive:
            archive.extractall(self.released, filter='data')
        original = json.loads((self.released / 'baseline.json').read_text())
        self.state = copy.deepcopy(original)
        self.state['packages'] = {self.name: original['packages'][self.name]}
        self.state['configurations'] = {'postgres': {self.name: []}}
        self.baseline = archive_state(self.released, self.state, root / 'active.tar.gz')
        self.candidate = root / 'candidate'
        self.candidate.mkdir()
        source = self.released / self.state['packages'][self.name]
        destination = self.candidate / self.state['packages'][self.name]
        shutil.copytree(source, destination)
        self.package = destination
        self.lib = destination / 'src/lib.rs'
        self.original_api = self.lib.read_text()
        self.manifest = destination / 'Cargo.toml'
        self.original_manifest = self.manifest.read_text()
        self.current = copy.deepcopy(self.state)
        self.current['source_revision'] = 'explicit-fixture-candidate'
        self.descriptor = self.candidate / 'coverage.json'
        self.policy = root / 'policy.toml'

    def write(self):
        self.descriptor.write_text(json.dumps(self.current))
        text = ''
        for product, mapping in self.current['configurations'].items():
            text += f'[feature-set.{product}]\n'
            text += ''.join(f'{name} = {json.dumps(features)}\n' for name, features in mapping.items())
        self.policy.write_text(text)

    def add_feature(self):
        self.manifest.write_text(self.original_manifest.replace('[features]', '[features]\nunreleased = []'))
        self.lib.write_text(self.original_api + '\n/// Added opt-in API.\n#[cfg(feature="unreleased")]\npub fn lifecycle_added() {}\n')
        self.current['configurations']['postgres'][self.name] = ['unreleased']
        self.write()

    def break_released_api(self):
        self.lib.write_text(self.lib.read_text().replace('pub use self::{', 'use self::{'))

    def adopt(self):
        self.baseline = archive_state(self.candidate, self.current, self.root / 'adopted.tar.gz')

    def break_added_api(self):
        self.lib.write_text(self.lib.read_text().replace('pub fn lifecycle_added() {}', 'pub fn lifecycle_added(_: u8) {}'))
