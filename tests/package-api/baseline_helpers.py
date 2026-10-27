"""Heavy historical fixture: extract the explicit seed and build all its APIs.

Owned by test_compare.py through xbuild's workspace/nextest integration test.
Preparation may fetch registry dependencies; production comparisons stay offline.
No historical workspace or ambient baseline is discovered.
"""
import hashlib
import json
from pathlib import Path
import tarfile

import compare

SEED_SHA256 = "7985670a095dc6b696edc471f9dbfdbc5947f7d5fcebcb960e68412b0cf91cec"
SOURCE_REVISION = "bd53b083b028dc8423f6c4e5b232cbd3c851c8c1"


def extract_approved_baseline(root, fixture, target):
    archive_path = root / "tests/package-api/fixtures/issue826-baseline.tar.gz"
    if hashlib.sha256(archive_path.read_bytes()).hexdigest() != SEED_SHA256:
        raise ValueError("approved baseline fixture checksum mismatch")
    source = fixture / "released"
    with tarfile.open(archive_path) as archive:
        archive.extractall(source, filter="data")
    state = json.loads((source / "baseline.json").read_text())
    if state['source_revision'] != SOURCE_REVISION or state['target'] != target:
        raise ValueError("unexpected approved baseline revision or target")
    return state, {name: source / relative for name, relative in state['packages'].items()}


def released_selections(root, fixture, toolchain, target):
    state, packages = extract_approved_baseline(root, fixture, target)
    env = compare.tool_environment(toolchain, fixture)
    for name, package in packages.items():
        preparation = fixture / (name + "-preparation")
        paths = [package, *(path for other, path in packages.items() if other != name)]
        staged = compare.prepare_side(paths, preparation)
        manifest = staged[0][1] / "Cargo.toml"
        # Fixture provisioning only: keep network resolution outside comparison.
        compare.run([env['CARGO'], 'fetch', '--manifest-path', str(manifest)], preparation, env)
        metadata = json.loads(compare.run(
            [env['CARGO'], 'metadata', '--offline', '--format-version=1',
             '--filter-platform', target, '--manifest-path', str(manifest)], preparation, env).stdout)
        selected = next(p for p in metadata['packages'] if Path(p['manifest_path']) == manifest)
        if not any('lib' in t['kind'] or 'proc-macro' in t['kind'] for t in selected['targets']):
            yield name, None
            continue
        resolved = {p['manifest_path'] for p in metadata['packages']}
        dependencies = [str(original) for original, (_, copied) in zip(paths[1:], staged[1:])
                        if str(copied / 'Cargo.toml') in resolved]
        yield name, dict(package=str(package), dependencies=dependencies,
                         features=state['configurations']['postgres'][name],
                         default_features=state['default_features'])
