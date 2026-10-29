"""Repository additive-feature lifecycle orchestration, using the #357 comparator."""
import argparse
import json
from pathlib import Path
import subprocess
import tarfile
import tempfile
import tomllib

from compare import AnalysisError, LIMITS, explicit_path, prepare_side, require, run, tool_environment

from resources import read_baseline

LEVELS = {'patch': 0, 'minor': 1, 'major': 2}


def inventory(manifest):
    features = set(manifest.get('features', {}))
    explicit_dependencies = {f[4:] for values in manifest.get('features', {}).values()
                             for f in values if f.startswith('dep:')}
    for table in [manifest, *manifest.get('target', {}).values()]:
        for kind in ('dependencies', 'build-dependencies'):
            for name, dependency in table.get(kind, {}).items():
                if isinstance(dependency, dict) and dependency.get('optional') and name not in explicit_dependencies:
                    features.add(name)
    return features


def read_state(path, policy=None):
    state = json.loads(path.read_text())
    require(isinstance(state, dict), 'coverage descriptor must be an object')
    if state.get('schema') == 2:
        from resources import read_resource_state
        result = read_resource_state(path, state)
        if policy is not None:
            document = tomllib.loads(policy.read_text())
            require(set(document) == {'feature-set'} and document['feature-set'] == state['configurations'],
                    'candidate resource configurations must match the selected policy')
        return result
    require(state.get('schema') == 1, 'unsupported coverage descriptor schema')
    require(isinstance(state.get('source_revision'), str) and state['source_revision'], 'source revision required')
    if 'source' in state:
        provenance = state['source']
        require(isinstance(provenance, dict) and provenance.get('mode') in ('commit', 'working-tree') and
                provenance.get('revision') == state['source_revision'] and type(provenance.get('dirty')) is bool,
                'invalid source provenance')
        require(provenance['mode'] != 'commit' or not provenance['dirty'], 'commit source cannot be dirty')
    require(isinstance(state.get('target'), str) and state['target'], 'explicit target required')
    require(state.get('default_features') is True, 'repository configurations require enabled defaults')
    require(isinstance(state.get('packages'), dict) and state['packages'], 'package set must not be empty')
    packages = {}
    for name, relative in state['packages'].items():
        require(isinstance(relative, str) and relative, 'package path required')
        location = (path.parent / relative).resolve(strict=True)
        require(location.is_relative_to(path.parent.resolve()), 'package must be inside supplied descriptor directory')
        manifest = tomllib.loads((location / 'Cargo.toml').read_text())
        require(manifest.get('package', {}).get('name') == name, f'package identity mismatch: {name}')
        require(manifest['package'].get('publish') is not False and manifest['package'].get('publish') != [],
                f'private package in coverage: {name}')
        require(isinstance(manifest['package'].get('version'), str), 'packaged version required')
        packages[name] = dict(path=location, manifest=manifest, features=inventory(manifest))
    if policy is not None:
        document = tomllib.loads(policy.read_text())
        require(set(document) == {'feature-set'}, 'expected only feature-set policy tables')
        state['configurations'] = document['feature-set']
    configurations = state.get('configurations')
    require(isinstance(configurations, dict) and configurations, 'configuration policy must not be empty')
    for product, mapping in configurations.items():
        require(isinstance(product, str) and product and isinstance(mapping, dict), 'invalid configuration')
        require(set(mapping) == set(packages), f'incomplete/unknown package mappings in {product}')
        for name, features in mapping.items():
            require(isinstance(features, list) and all(isinstance(f, str) and f for f in features),
                    f'features must be string lists: {product}/{name}')
            require(set(features) <= packages[name]['features'], f'unknown feature: {product}/{name}')
    return state, packages


def dependency_closure(name, packages):
    selected = set()
    pending = [name]
    while pending:
        manifest = packages[pending.pop()]['manifest']
        for table in [manifest, *manifest.get('target', {}).values()]:
            for kind in ('dependencies', 'build-dependencies', 'dev-dependencies'):
                for alias, dependency in table.get(kind, {}).items():
                    actual = dependency.get('package', alias) if isinstance(dependency, dict) else alias
                    if actual in packages and actual != name and actual not in selected:
                        selected.add(actual)
                        pending.append(actual)
    return [str(packages[n]['path']) for n in sorted(selected)]


def selection(name, features, packages):
    if 'resources' in packages[name]:
        key = tuple(sorted(features))
        require(key in packages[name]['resources'], f'missing released API resource: {name}/{features}')
        return packages[name]['resources'][key]
    return dict(package=str(packages[name]['path']), dependencies=dependency_closure(name, packages),
                features=sorted(set(features)), default_features=True)


def resolve_dependencies(side, target, toolchain, context):
    """Let Cargo select active local dependencies; reuse comparator staging."""
    if side is None or 'rustdoc' in side or not side['dependencies']:
        return side
    with tempfile.TemporaryDirectory(prefix='coverage-dependencies-') as directory:
        scratch = Path(directory)
        env = tool_environment(toolchain, context)
        original = [Path(side['package']), *map(Path, side['dependencies'])]
        staged = scratch / 'packages'
        packages = prepare_side(original, staged)
        command = [env['CARGO'], 'metadata', '--format-version=1', '--config', str(staged / '.cargo/config.toml'),
                   '--filter-platform', target, '--manifest-path', str(packages[0][1] / 'Cargo.toml')]
        if side['features']:
            command += ['--features', ','.join(side['features'])]
        metadata = json.loads(run(command, context, env).stdout)
        resolved = {p['manifest_path'] for p in metadata['packages']}
        resolved_names = {p['name'] for p in metadata['packages']}
        for package, location in packages[1:]:
            require(str(location / 'Cargo.toml') in resolved or package['name'] not in resolved_names,
                    f'supplied local dependency resolved from a different source: {package["name"]}')
        selected = [str(source) for source, (_, location) in zip(original[1:], packages[1:])
                    if str(location / 'Cargo.toml') in resolved]
        return dict(side, dependencies=selected)


def has_api(package):
    if 'has_api' in package:
        return package['has_api']
    manifest = package['manifest']
    return manifest.get('lib') is not None or (manifest['package'].get('autolib', True) and
                                                (package['path'] / 'src/lib.rs').is_file())


def schedule(old, old_packages, new, new_packages):
    """Keep old obligations; names do not freeze mappings or invent predecessors."""
    calls, removals, additions, binaries = [], [], [], []
    common = set(old_packages) & set(new_packages)
    for name in sorted(common):
        removed = old_packages[name]['features'] - new_packages[name]['features']
        if removed:
            removals.append(dict(package=name, reason='released-manifest-features-removed', features=sorted(removed)))
    seen = set()
    current_selections = set()

    def compare_pair(name, product, before, after, kind):
        if not has_api(new_packages[name]):
            if name in old_packages and has_api(old_packages[name]):
                removals.append(dict(package=name, reason='released-api-target-removed'))
            binaries.append(dict(package=name, configuration=product, reason='binary-only'))
            return
        if before is not None and not has_api(old_packages[name]):
            additions.append(dict(package=name, configuration=product, reason='new-api-target'))
            before = None
        new_side = selection(name, after, new_packages)
        current_selections.add(json.dumps(new_side, sort_keys=True))
        old_side = selection(name, before, old_packages) if before is not None else None
        key = json.dumps([old_side, new_side], sort_keys=True)
        if key not in seen:
            seen.add(key)
            calls.append(dict(package=name, configuration=product, kind=kind, old=old_side, new=new_side))

    for product, mapping in old['configurations'].items():
        current = new['configurations'].get(product)
        for name, before in mapping.items():
            if name not in common:
                continue
            if current is None or not set(before) <= set(current[name]):
                removals.append(dict(package=name, configuration=product, reason='released-configured-coverage-removed'))
            # Manifest removal is already successful Major evidence, not a request
            # to generate an impossible historical selection on the candidate.
            if not set(before) <= new_packages[name]['features']:
                continue
            compare_pair(name, product, before, before, 'released')
            if current is not None and set(before) < set(current[name]):
                compare_pair(name, product, before, current[name], 'expanded')
    for product, mapping in new['configurations'].items():
        for name, features in mapping.items():
            if name not in old_packages:
                # No released crate means no SemVer contract to compare or bump.
                additions.append(dict(package=name, configuration=product, reason='new-package'))
                continue
            if product not in old['configurations']:
                released_selection = any(
                    set(previous[name]) == set(features) for previous in old['configurations'].values())
                additions.append(dict(package=name, configuration=product, reason='new-coverage',
                                      released_selection=released_selection))
            if not has_api(new_packages[name]):
                continue
            current = selection(name, features, new_packages)
            if json.dumps(current, sort_keys=True) not in current_selections:
                # Removal evidence must not hide an invalid replacement/shrink.
                # This validates an existing released crate's current selection;
                # it does not invent a historical contract for that selection.
                kind = 'new-coverage' if product not in old['configurations'] else 'current-validation'
                compare_pair(name, product, None, features, kind)
    binaries.extend(dict(package=name, reason="no-longer-publishable")
                    for name in sorted(set(old_packages) - set(new_packages)))
    binaries.extend(dict(package=name, reason="never-published")
                    for name in sorted(set(new_packages) - set(old_packages)))
    return calls, removals, additions, binaries


def compare_selection(call, target, toolchain, comparator, wrapper, semver, scratch, index, context, resource_output=None):
    try:
        request = dict(old=resolve_dependencies(call['old'], target, toolchain, context),
                       new=resolve_dependencies(call['new'], target, toolchain, context),
                       target=target, toolchain=toolchain, cargo_context=str(context))
        if resource_output is not None:
            request['resource_output'] = str(resource_output)
        request_path = scratch / f'comparison-{index}.json'
        request_path.write_text(json.dumps(request))
        completed = subprocess.run([str(comparator), '--request', str(request_path), '--diff-wrapper', str(wrapper),
                                    '--semver-checks', str(semver)], text=True, capture_output=True)
        try:
            report = json.loads(completed.stdout)
        except ValueError as error:
            raise AnalysisError(f'{call["package"]}/{call["configuration"]}: invalid comparator report '
                                f'(exit {completed.returncode}): {completed.stderr}') from error
        require(completed.returncode == 0 and report.get('status') == 'detected' and report.get('impact') in LEVELS,
                f'{call["package"]}/{call["configuration"]}/{call["kind"]}: {report}')
        return report
    except AnalysisError as error:
        raise AnalysisError(f'{call["package"]}/{call["configuration"]}/{call["kind"]}: {error}') from error


def execute(old_path, new_path, policy, comparator, wrapper, semver, toolchain, scratch, cargo_context=None, resource_directory=None):
    context = cargo_context or Path.cwd()
    old, old_packages = read_baseline(old_path)
    new, new_packages = read_state(new_path, policy)
    require(old['target'] == new['target'], 'baseline/candidate targets differ')
    calls, removed, added, binaries = schedule(old, old_packages, new, new_packages)
    reports = []
    new_surface = any(addition['reason'] != 'new-package' and
                      not addition.get('released_selection', False) for addition in added)
    impact = 'major' if removed else ('minor' if new_surface else 'patch')
    retained = {}
    for index, call in enumerate(calls):
        resource_output = None
        if resource_directory is not None:
            key = json.dumps(call['new'], sort_keys=True)
            if key in retained:
                call = dict(call, new=retained[key])
            resource_output = resource_directory / str(index)
        arguments = (call, new['target'], toolchain, comparator, wrapper, semver, scratch, index, context)
        if resource_output is None:
            report = compare_selection(*arguments)
        else:
            report = compare_selection(*arguments, resource_output=resource_output)
        if resource_output is not None:
            retained[key] = json.loads((resource_output / 'resource.json').read_text())
        coverage = report['coverage']
        lost = set(coverage['old']['resolved_features']) - set(coverage['new']['resolved_features'])
        if lost:
            removed.append(dict(package=call['package'], configuration=call['configuration'],
                                reason='released-resolved-coverage-removed', features=sorted(lost)))
            impact = 'major'
        if LEVELS[report['impact']] > LEVELS[impact]:
            impact = report['impact']
        reports.append(dict(package=call['package'], configuration=call['configuration'], kind=call['kind'], result=report))
    return dict(status='detected', impact=impact, comparisons=reports, removed_coverage=removed,
                new_coverage=added, excluded=binaries, next_coverage=new, limits=LIMITS)


def unpack_baseline(path, destination):
    with tarfile.open(path) as archive:
        members = archive.getmembers()
        require(all(m.isfile() or m.isdir() for m in members), 'baseline links/special files forbidden')
        names = [str(Path(m.name)) for m in members]
        require(len(names) == len(set(names)), 'duplicate baseline archive members')
        require(all(not Path(name).is_absolute() and '..' not in Path(name).parts for name in names),
                'baseline archive paths must be relative and contained')
        archive.extractall(destination, filter='data')
    descriptor = destination / 'baseline.json'
    read_baseline(descriptor)
    return descriptor


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--baseline', required=True, help='absolute active baseline archive path')
    parser.add_argument('--candidate', required=True, help='absolute candidate descriptor path')
    parser.add_argument('--policy', required=True, help='absolute current TOML policy path')
    parser.add_argument('--diff-wrapper', required=True)
    parser.add_argument('--semver-checks', required=True)
    parser.add_argument('--toolchain', required=True)
    args = parser.parse_args()
    try:
        baseline, candidate, policy, wrapper, semver = [explicit_path(p) for p in
            (args.baseline, args.candidate, args.policy, args.diff_wrapper, args.semver_checks)]
        with tempfile.TemporaryDirectory(prefix='coverage-lifecycle-') as directory:
            scratch = Path(directory)
            unpacked = scratch / 'baseline'
            unpack_baseline(baseline, unpacked)
            report = execute(unpacked / 'baseline.json', candidate, policy,
                             Path(__file__).resolve().parents[1] / 'compare-package-api',
                             wrapper, semver, args.toolchain, scratch)
    except (AnalysisError, OSError, ValueError, KeyError, TypeError, AttributeError, EOFError, tarfile.TarError) as error:
        print(json.dumps(dict(status='analysis-error', diagnostics=str(error), limits=LIMITS)))
        return 1
    print(json.dumps(report))
    return 0
