"""Repository source capture and shared would-be-release package production."""
import json
import os
from pathlib import Path
import re
import shutil
import tarfile
import time
import tomllib

from compare import require, run
from lifecycle import LEVELS, read_state, unpack_baseline

TOOLS = Path(__file__).resolve().parents[2]


def repository_environment():
    # A caller's Git index/worktree overrides must never address the original
    # index while preparing a scratch clone. Build outputs belong to scratch too.
    return {k: v for k, v in os.environ.items()
            if not k.startswith('GIT_') and k not in ('CARGO_TARGET_DIR', 'CARGO_BUILD_TARGET_DIR')}


def git(repository, *arguments):
    return run(['git', '-c', 'core.hooksPath=/dev/null', *arguments], repository,
               repository_environment()).stdout


def capture_source(repository, destination, revision):
    """Clone one explicit commit, or overlay the current on-disk working tree."""
    selected = revision if revision is not None else 'HEAD'
    if revision is not None:
        require(re.fullmatch(r'[0-9a-f]{40}', revision) is not None, 'revision must be a full commit SHA')
    head = git(repository, 'rev-parse', '--verify', selected + '^{commit}').strip()
    run(['git', '-c', 'core.hooksPath=/dev/null', 'clone', '--quiet', '--shared', '--no-checkout',
         '--', str(repository), str(destination)], repository, repository_environment())
    git(destination, 'checkout', '--quiet', '--detach', head)
    require(not any(p.is_symlink() for p in destination.rglob('*') if '.git' not in p.relative_to(destination).parts),
            'source symlinks are not supported by isolated materialization')
    if revision is not None:
        return dict(mode='commit', revision=head, dirty=False)

    dirty = overlay_working_tree(repository, destination)
    return dict(mode='working-tree', revision=head, dirty=dirty)


def overlay_working_tree(repository, destination):
    # Keep Git's ignore rules in the scratch repository. Enumerate ignored input
    # files too: Cargo's explicit include list can select an ignored source file.
    # Cargo always excludes target directories and .git; linked task worktrees
    # are repository administration, never neighboring source inputs.
    paths = git(repository, 'ls-files', '--cached', '--others', '-z', '--', '.',
                ':(exclude)target', ':(exclude).worktrees').split('\0')
    tracked = git(destination, 'ls-files', '-z').split('\0')
    for relative in tracked:
        if relative and not (repository / relative).is_file():
            path = destination / relative
            if path.is_file() or path.is_symlink():
                path.unlink()
    for relative in set(paths) - {''}:
        source, target = repository / relative, destination / relative
        if not source.exists() and not source.is_symlink():
            continue
        require(not source.is_symlink(), f'source symlinks are not supported: {relative}')
        if source.is_dir():
            # Gitlinks are not ordinary package files. Uninitialized submodules
            # remain uninitialized, just as in the committed source operation.
            continue
        target.parent.mkdir(parents=True, exist_ok=True)
        if target.is_dir():
            shutil.rmtree(target)
        elif target.is_symlink() or target.exists():
            target.unlink()
        shutil.copy2(source, target)
    exclude = git(repository, 'rev-parse', '--git-path', 'info/exclude').strip()
    exclude = Path(exclude) if Path(exclude).is_absolute() else repository / exclude
    if exclude.is_file():
        (destination / '.git/info/exclude').write_bytes(exclude.read_bytes())
    dirty = bool(git(destination, 'status', '--porcelain', '--untracked-files=all'))
    # Ignored files may be included by Cargo even though Git status omits them.
    ignored = git(destination, 'ls-files', '--others', '--ignored', '--exclude-standard', '-z')
    return dirty or bool(ignored)


def publication_metadata(source, env):
    metadata = json.loads(run(['cargo', 'metadata', '--locked', '--no-deps', '--format-version=1'], source, env).stdout)
    packages = {p['name']: p for p in metadata['packages']
                if p['id'] in metadata['workspace_members'] and p['source'] is None and p['publish'] != []}
    require(packages, 'no publishable workspace packages')
    return metadata, packages


def release_intent(metadata, checked):
    intent = (metadata.get('metadata') or {}).get('next-release')
    require(isinstance(intent, dict), 'missing workspace.metadata.next-release')
    for name in sorted(checked):
        level = intent.get(name)
        require(isinstance(level, str) and level in LEVELS,
                f'missing/invalid workspace.metadata.next-release intent for {name}: {level!r}')
    return {name: intent[name] for name in sorted(checked)}


def validate_policy(source, packages):
    policy = tomllib.loads((source / '.local/semver-feature-sets.toml').read_text())
    require(set(policy) == {'feature-set'}, 'expected only feature-set policy tables')
    configurations = policy['feature-set']
    require(isinstance(configurations, dict) and configurations, 'configuration policy must not be empty')
    for product, mapping in configurations.items():
        require(isinstance(mapping, dict) and set(mapping) == set(packages),
                f'incomplete/unknown package mappings in {product}')
        for name, features in mapping.items():
            require(isinstance(features, list) and all(isinstance(f, str) and f for f in features),
                    f'features must be string lists: {product}/{name}')
            require(set(features) <= set(packages[name]['features']), f'unknown feature: {product}/{name}')
    return configurations


def prepare_input(repository, revision, scratch, toolchain, target):
    source = scratch / 'source'
    provenance = capture_source(repository, source, revision)
    env = repository_environment()
    env.update(RUSTUP_TOOLCHAIN=toolchain, CARGO_TARGET_DIR=str(scratch / 'target'),
               SQL_FUN_HOME=str(source / 'sql-fun-home'))
    metadata, packages = publication_metadata(source, env)
    baseline = unpack_baseline(source / '.local/semver_baseline.tar.gz', scratch / 'baseline')
    old, _ = read_state(baseline)
    require(old['target'] == target, 'baseline/candidate targets differ')
    intent = release_intent(metadata, set(packages) & set(old['packages']))
    configurations = validate_policy(source, packages)
    return source, provenance, env, baseline, intent, configurations


def package_snapshot(source, provenance, env, configurations, target, output):
    """Retain the complete candidate, independently of the verdict scope."""
    require(not output.exists(), 'snapshot output must not already exist')
    started = time.monotonic()
    run([str(TOOLS / 'bin/materialize-release-versions'), '--release'], source, env)
    manifest = run([str(TOOLS / 'sbin/package-release-sources')], source, env).stdout
    records = {}
    output.mkdir(parents=True)
    try:
        for row in manifest.splitlines():
            name, version, _ = row.split('\t')
            archive_path = Path(env['CARGO_TARGET_DIR']) / 'package' / f'{name}-{version}.crate'
            with tarfile.open(archive_path) as archive:
                require(all(m.isfile() or m.isdir() for m in archive.getmembers()),
                        f'package links/special files forbidden: {name}')
                archive.extractall(output / 'packages', filter='data')
            records[name] = f'packages/{name}-{version}'
        descriptor = dict(schema=1, source_revision=provenance['revision'], source=provenance,
                          target=target, default_features=True, packages=records, configurations=configurations)
        path = output / 'coverage.json'
        path.write_text(json.dumps(descriptor, indent=2) + '\n')
        read_state(path)
        return path, time.monotonic() - started
    except Exception:
        shutil.rmtree(output)
        raise
