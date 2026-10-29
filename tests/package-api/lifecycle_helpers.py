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


def write_resource_baseline(root, source_state):
    """Represent released coverage without retaining buildable package sources."""
    import hashlib
    import json
    import sys
    sys.path.insert(0, str(ROOT / '.local/sbin/package-api'))
    from lifecycle import has_api, read_state
    path = root / 'coverage.json'
    state, packages = read_state(path)
    records = {}
    for name, package in packages.items():
        record = dict(version=package['manifest']['package']['version'], features=sorted(package['features']),
                      has_api=bool(has_api(package)), selections=[])
        records[name] = record
        if not record['has_api']:
            continue
        choices = {tuple(sorted(mapping[name])) for mapping in source_state['configurations'].values()}
        for index, features in enumerate(sorted(choices)):
            artifact = root / f'{name}-{index}.json'
            artifact.write_text(json.dumps(dict(format_version=61, root='0', index={'0': {'name': name.replace('-', '_')}},
                crate_version='1.0.0', includes_private=False, target={'triple': source_state['target']})))
            coverage = dict(name=name, version=record['version'], features=list(features), default_features=True,
                            resolved_features=list(features), feature_inventory=record['features'], schema=61,
                            documented_target=source_state['target'], target_kind=['lib'])
            record['selections'].append(dict(rustdoc=artifact.name, coverage=coverage,
                producer=dict(target=source_state['target'], host=source_state['target'], toolchain='fixture',
                              rustc='rustc fixture', cargo='cargo fixture', rustdoc='rustdoc fixture'), sha256=hashlib.sha256(artifact.read_bytes()).hexdigest()))
    result = dict(state, schema=2, packages=records)
    descriptor = root / 'baseline.json'
    descriptor.write_text(json.dumps(result))
    return descriptor
