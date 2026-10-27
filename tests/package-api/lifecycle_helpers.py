"""Small source-package fixtures; no compiler or analyzer setup."""
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


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
