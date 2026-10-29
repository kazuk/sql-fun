"""Repository-local release-intent validation and optional candidate archive."""
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
from repository import TOOLS, package_snapshot, prepare_input, repository_environment
from resources import produce_resources
from baseline_archive import write_candidate


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
    output = args.candidate_output
    if output is not None:
        require(args.revision is not None, 'candidate output requires explicit commit mode (--revision)')
        require(output.is_absolute() and not output.exists(), 'new absolute candidate output required')
        require(not output.resolve().is_relative_to(explicit_path(args.repository)),
                'candidate output must be outside the selected repository')
    resources = scratch / 'retained' if output is not None else None
    source, provenance, env, baseline, intent, configurations = prepare_input(
        explicit_path(args.repository), args.revision, scratch, args.toolchain, args.target)
    wrapper, semver = prepare_tools(args.toolchain, scratch)
    candidate, package_seconds = package_snapshot(source, provenance, env, configurations, args.target, scratch / 'candidate')
    analysis_started = time.monotonic()
    report = execute(baseline, candidate, source / '.local/semver-feature-sets.toml',
                     TOOLS / 'sbin/compare-package-api', wrapper, semver, args.toolchain, scratch, explicit_path(args.repository), resource_directory=resources)
    results = intent_results(report, intent)
    passed = all(r['outcome'] == 'pass' for r in results)
    if passed and output is not None:
        descriptor = produce_resources(candidate, scratch / 'bundle', args.toolchain, explicit_path(args.repository), resources)
        write_candidate(descriptor, output)
        report['candidate'] = str(output)
    report.update(status='passed' if passed else 'exceeds-intent', source=provenance, target=args.target,
                  toolchain=args.toolchain, crates=results,
                  timings=dict(packaging_seconds=package_seconds, analysis_seconds=time.monotonic() - analysis_started,
                               total_seconds=time.monotonic() - started))
    # Package paths expire with scratch; the optional archive retains resources.
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
    parser = arguments(__doc__)
    parser.add_argument('--candidate-output', type=Path, help='new absolute candidate archive outside the repository; commit mode only')
    args = parser.parse_args()
    try:
        with tempfile.TemporaryDirectory(prefix='semver-intent-') as directory:
            report = validate(args, Path(directory))
    except (AnalysisError, OSError, ValueError, KeyError, TypeError, AttributeError, EOFError, tarfile.TarError) as error:
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
