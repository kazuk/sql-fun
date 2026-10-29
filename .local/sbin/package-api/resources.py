"""Immutable comparison resources: no source checkout or old-side build."""
import hashlib
import json
from pathlib import Path
import shutil
import tempfile

from compare import current_api, require, saved_resource


def contained_file(root, relative):
    require(isinstance(relative, str) and relative and not Path(relative).is_absolute() and
            '..' not in Path(relative).parts, 'relative contained resource path required')
    path = (root / relative).resolve(strict=True)
    require(path.is_relative_to(root.resolve()) and path.is_file(), 'resource must be inside the baseline')
    return path


def read_baseline(path):
    return read_resource_state(path, json.loads(path.read_text()))


def read_resource_state(path, state):
    require(isinstance(state, dict), 'baseline descriptor must be an object')
    require(state.get('schema') == 2, 'baseline must contain generated comparison resources (schema 2); no source rebuild fallback')
    require(state.get('default_features') is True, 'resource baseline requires enabled defaults')
    require(isinstance(state.get('source_revision'), str) and state['source_revision'], 'source revision required')
    require(isinstance(state.get('target'), str) and state['target'], 'explicit target required')
    require(isinstance(state.get('packages'), dict) and state['packages'], 'package inventory required')
    if 'source' in state:
        source = state['source']
        require(isinstance(source, dict) and source.get('mode') in ('commit', 'working-tree') and
                source.get('revision') == state['source_revision'] and type(source.get('dirty')) is bool and
                (source['mode'] != 'commit' or not source['dirty']), 'invalid source provenance')
    packages = {}
    for name, record in state['packages'].items():
        require(isinstance(name, str) and name and '/' not in name and name not in ('.', '..'), 'invalid package name')
        require(isinstance(record, dict) and isinstance(record.get('version'), str), f'invalid package inventory: {name}')
        require(type(record.get('has_api')) is bool and isinstance(record.get('features'), list) and
                all(isinstance(f, str) and f for f in record['features']), f'invalid package features/target: {name}')
        require(isinstance(record.get('selections'), list), f'resource selections required: {name}')
        selections = {}
        for resource in record['selections']:
            coverage = resource['coverage']
            features = coverage['features']
            require(isinstance(features, list) and all(isinstance(f, str) and f for f in features),
                    f'invalid selected features: {name}')
            require(coverage['name'] == name and coverage['version'] == record['version'] and
                    coverage['default_features'] is True and coverage['feature_inventory'] == sorted(record['features']) and
                    set(features) <= set(record['features']) and isinstance(coverage['resolved_features'], list),
                    f'inconsistent resource coverage: {name}')
            artifact = contained_file(path.parent, resource['rustdoc'])
            require(hashlib.sha256(artifact.read_bytes()).hexdigest() == resource['sha256'], f'resource digest mismatch: {name}')
            key = tuple(sorted(features))
            require(key not in selections, f'duplicate resource selection: {name}')
            require(isinstance(resource['producer'], dict) and resource['producer'].get('target') == state['target'],
                    f'missing/inconsistent producer: {name}')
            selections[key] = dict(rustdoc=str(artifact), coverage=coverage, producer=resource['producer'])
            validate_producer(resource['producer'], coverage, state['target'])
            saved_resource(selections[key], state['target'])
        require(record['has_api'] or not selections, f'binary-only package has API resources: {name}')
        packages[name] = dict(features=set(record['features']), has_api=record['has_api'], resources=selections)
    configurations = state.get('configurations')
    require(isinstance(configurations, dict) and configurations, 'configuration inventory required')
    for product, mapping in configurations.items():
        require(isinstance(product, str) and product, 'invalid product name')
        require(isinstance(mapping, dict) and set(mapping) == set(packages), f'incomplete resource mapping: {product}')
        for name, features in mapping.items():
            require(isinstance(features, list) and all(isinstance(f, str) for f in features) and
                    set(features) <= packages[name]['features'], f'invalid resource features: {product}/{name}')
            require(not packages[name]['has_api'] or tuple(sorted(features)) in packages[name]['resources'],
                    f'missing comparison resource: {product}/{name}')
    return state, packages


def validate_producer(producer, coverage, target):
    require(all(isinstance(producer.get(field), str) and producer[field]
                for field in ('toolchain', 'rustc', 'cargo', 'rustdoc', 'host')),
            'incomplete resource producer')
    kinds = coverage.get('target_kind')
    require(isinstance(kinds, list) and kinds and all(isinstance(kind, str) for kind in kinds),
            'resource target kind required')
    documented = producer['host'] if 'proc-macro' in kinds else target
    require(coverage.get('documented_target') == documented, 'resource documented target mismatch')
    require(type(coverage.get('schema')) is int and coverage['schema'] > 0, 'invalid resource JSON schema')
    require(all(isinstance(feature, str) and feature for feature in coverage['resolved_features']),
            'invalid resolved features')


def produce_resources(descriptor, output, toolchain, context, retained=None):
    """Complete current configured resources, reusing retained comparison outputs."""
    from lifecycle import has_api, read_state, resolve_dependencies, selection
    state, packages = read_state(descriptor)
    require(state['schema'] == 1, 'resource production requires candidate source packages')
    require(not output.exists(), 'resource output must not already exist')
    available = {}
    if retained is not None:
        for path in sorted(retained.glob('*/resource.json')):
            resource = json.loads(path.read_text())
            saved_resource(resource, state['target'])
            coverage = resource['coverage']
            available[(coverage['name'], tuple(sorted(coverage['features'])))] = resource
    output.mkdir(parents=True)
    try:
        records = {}
        with tempfile.TemporaryDirectory(prefix='semver-resource-production-') as directory:
            scratch = Path(directory)
            for name, package in sorted(packages.items()):
                record = dict(version=package['manifest']['package']['version'], features=sorted(package['features']),
                              has_api=bool(has_api(package)), selections=[])
                records[name] = record
                if not record['has_api']:
                    continue
                choices = sorted({tuple(sorted(mapping[name])) for mapping in state['configurations'].values()})
                for index, features in enumerate(choices):
                    side = available.get((name, features))
                    if side is None:
                        side = resolve_dependencies(selection(name, features, packages), state['target'], toolchain, context)
                    artifact, coverage, producer, _ = current_api(side, state['target'], toolchain, context,
                                                                 scratch / f'{name}-{index}')
                    relative = f'rustdoc/{name}/{index}.json'
                    destination = output / relative
                    destination.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copy2(artifact, destination)
                    record['selections'].append(dict(rustdoc=relative, coverage=coverage, producer=producer,
                        sha256=hashlib.sha256(destination.read_bytes()).hexdigest()))
        result = dict(schema=2, source_revision=state['source_revision'], target=state['target'], default_features=True,
                      packages=records, configurations=state['configurations'])
        if 'source' in state:
            result['source'] = state['source']
        path = output / 'baseline.json'
        path.write_text(json.dumps(result, indent=2) + '\n')
        read_resource_state(path, result)
        return path
    except Exception:
        shutil.rmtree(output)
        raise


def retain_resource(output, artifact, coverage, producer):
    """Preserve current JSON before the comparator's scratch directory expires."""
    require(output.is_absolute() and not output.exists(), 'new absolute resource output required')
    output.mkdir(parents=True)
    destination = output / 'api.json'
    shutil.copyfile(artifact, destination)
    resource = dict(rustdoc=str(destination), coverage=coverage, producer=producer)
    (output / 'resource.json').write_text(json.dumps(resource))
