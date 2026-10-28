"""Repository-local release-intent validation and reusable snapshot entry point."""
import argparse
import json
from pathlib import Path
import re
import shutil
import sys
import tarfile
import tempfile
import time

from compare import AnalysisError, LIMITS, explicit_path, require, run
from lifecycle import LEVELS, execute
from repository import TOOLS, package_snapshot, prepare_input, prepare_source, repository_environment
from resources import produce_resources


def intent_results(report, intent):
    """Apply evidence per crate, without contaminating it with a workspace max."""
    evidence = {name: [] for name in intent}
    for comparison in report['comparisons']:
        evidence[comparison['package']].append(dict(configuration=comparison['configuration'],
            kind=comparison['kind'], impact=comparison['result']['impact']))
    for item in report['removed_coverage']:
        evidence[item['package']].append(dict(item, impact='major'))
    for item in report['new_coverage']:
        if item['reason'] != 'new-package' and not item.get('released_selection', False):
            evidence[item['package']].append(dict(item, impact='minor'))
    results = []
    for name, declared in intent.items():
        items = evidence[name]
        impact = max(['patch', *(item['impact'] for item in items)], key=LEVELS.__getitem__)
        results.append(dict(package=name, declared=declared, impact=impact,
                            outcome='pass' if LEVELS[impact] <= LEVELS[declared] else 'exceeds-intent', evidence=items))
    return results


def prepare_tools(toolchain, scratch):
    env = repository_environment()
    env['RUSTUP_TOOLCHAIN'] = toolchain
    semver = shutil.which('cargo-semver-checks')
    require(semver is not None, 'cargo-semver-checks 0.51.0-sqlfun.1 is required; use the documented builder image')
    require(run([semver, '--version'], scratch, env).stdout.strip() == 'cargo-semver-checks 0.51.0-sqlfun.1',
            'cargo-semver-checks 0.51.0-sqlfun.1 is required')
    root = TOOLS.parent
    # The wrapper runs here, independently of the API's compilation target.
    rustc = run([env.get('RUSTC', 'rustc'), '--version', '--verbose'], root, env).stdout
    host = re.search(r'^host: (\S+)$', rustc, re.M)
    require(host is not None, 'selected rustc did not report its host in --version --verbose')
    build = run(['cargo', 'build', '--locked', '-p', 'xbuild', '--bin', 'xbuild',
                 '--target', host[1], '--message-format=json'], root, env)
    artifacts = [json.loads(line) for line in build.stdout.splitlines()]
    executables = [Path(item['executable']) for item in artifacts
                   if item.get('reason') == 'compiler-artifact' and item.get('executable')
                   and item['target']['name'] == 'xbuild' and 'bin' in item['target']['kind']]
    require(len(executables) == 1 and executables[0].is_file(), 'Cargo did not report the built xbuild executable')
    return executables[0], Path(semver)


def validate(args, scratch):
    started = time.monotonic()
    source, provenance, env, baseline, intent, configurations = prepare_input(
        explicit_path(args.repository), args.revision, scratch, args.toolchain, args.target)
    wrapper, semver = prepare_tools(args.toolchain, scratch)
    candidate, package_seconds = package_snapshot(source, provenance, env, configurations, args.target, scratch / 'candidate')
    analysis_started = time.monotonic()
    report = execute(baseline, candidate, source / '.local/semver-feature-sets.toml',
                     TOOLS / 'sbin/compare-package-api', wrapper, semver, args.toolchain, scratch, explicit_path(args.repository))
    results = intent_results(report, intent)
    passed = all(r['outcome'] == 'pass' for r in results)
    report.update(status='passed' if passed else 'exceeds-intent', source=provenance, target=args.target,
                  toolchain=args.toolchain, crates=results,
                  timings=dict(packaging_seconds=package_seconds, analysis_seconds=time.monotonic() - analysis_started,
                               total_seconds=time.monotonic() - started))
    # The local operation owns temporary packages. Only the explicit snapshot
    # producer returns a descriptor whose relative package paths remain usable.
    report.pop('next_coverage')
    return report


def arguments(description):
    parser = argparse.ArgumentParser(description=description)
    parser.add_argument('--repository', required=True, help='absolute selected repository root')
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument('--working-tree', action='store_true', help='current staged/unstaged and Cargo-included untracked contents')
    mode.add_argument('--revision', help='explicit full commit SHA; never includes dirty working-tree contents')
    parser.add_argument('--toolchain', required=True, help='installed Rust toolchain for current API production')
    parser.add_argument('--target', required=True)
    return parser


def main():
    args = arguments(__doc__).parse_args()
    try:
        with tempfile.TemporaryDirectory(prefix='semver-intent-') as directory:
            report = validate(args, Path(directory))
    except (AnalysisError, OSError, ValueError, KeyError, TypeError, AttributeError, tarfile.TarError) as error:
        report = dict(status='analysis-error', diagnostics=str(error), limits=LIMITS)
    print(json.dumps(report))
    for crate in report.get('crates', []):
        print(f'{crate["package"]}: declared={crate["declared"]} detected={crate["impact"]} {crate["outcome"]}', file=sys.stderr)
        for evidence in crate['evidence']:
            print(f'  {evidence.get("configuration", "package-wide")}: {evidence.get("kind", evidence.get("reason"))} '
                  f'{evidence["impact"]}', file=sys.stderr)
    if 'diagnostics' in report:
        print(report['diagnostics'], file=sys.stderr)
    return 0 if report['status'] == 'passed' else 1


def snapshot_main():
    parser = arguments('Internal generated comparison-resource producer for #360')
    parser.add_argument('--output', required=True, type=Path, help='new absolute resource directory outside the selected repository')
    args = parser.parse_args()
    try:
        repository = explicit_path(args.repository)
        output = args.output.resolve()
        require(args.output.is_absolute() and not output.is_relative_to(repository),
                'snapshot output must be an absolute path outside the selected repository')
        require(not output.exists(), 'snapshot output must not already exist')
        with tempfile.TemporaryDirectory(prefix='semver-resources-') as directory:
            source, provenance, env, _, _, configurations = prepare_source(
                repository, args.revision, Path(directory), args.toolchain)
            started = time.monotonic()
            packages, _ = package_snapshot(source, provenance, env, configurations, args.target, Path(directory) / 'candidate')
            descriptor = produce_resources(packages, output, args.toolchain, repository)
            elapsed = time.monotonic() - started
            print(json.dumps(dict(descriptor=str(descriptor), resource_seconds=elapsed, source=provenance)))
        return 0
    except (AnalysisError, OSError, ValueError, KeyError, TypeError, AttributeError, tarfile.TarError) as error:
        print(json.dumps(dict(status='analysis-error', diagnostics=str(error))), file=sys.stderr)
        return 1
