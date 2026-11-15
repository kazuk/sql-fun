"""Profile one complete shared-server regression workload with perf."""
import argparse
import collections
from datetime import datetime, timezone
import json
import multiprocessing
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[3]
SUITE = ROOT / "tests/sqlast-regressions"
sys.path.insert(0, str(SUITE))
import parallel_runner as regression
import scheduling

from provision import provision
from server import IDLE_TIMEOUT, ProfiledServer
from tools import Tools, digest, run


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n")


def arguments(argv):
    parser = argparse.ArgumentParser(prog="profile-server-cpu", description=__doc__,
                                     formatter_class=argparse.ArgumentDefaultsHelpFormatter)
    parser.add_argument("--mode", choices=("record", "stat"), default="record",
                        help="One measurement per invocation")
    parser.add_argument("--features", required=True, choices=("default", "all"), help="Feature variant")
    parser.add_argument("--sql-fun-home", required=True, type=Path, help="Absolute builtin catalog installation")
    parser.add_argument("--output", type=Path, default=ROOT / "target/cpu-reports", help="Report parent directory")
    args = parser.parse_args(argv)
    if (sys.platform != "linux" or 'ID=ubuntu\n' not in Path("/etc/os-release").read_text()
            or 'VERSION_ID="24.04"' not in Path("/etc/os-release").read_text()
            or os.uname().machine != "x86_64"):
        parser.error("Automatic profiling tool provisioning supports Ubuntu 24.04 amd64 only")
    for name in ("sql_fun_home", "output"):
        path = getattr(args, name)
        if not path.is_absolute() or any(char in str(path) for char in '\n\r"\\%'):
            parser.error(f"--{name.replace('_', '-')} must be an absolute path without quotes, backslashes, percent signs or newlines")
    return args


def prerequisites(args):
    commands = ["cargo", "rustc", "git", "bash", "jq", "cc",
                "apt-get", "dpkg-deb", "dpkg-architecture"]
    for command in commands:
        if not shutil.which(command):
            raise RuntimeError(f"Required profiling tool is unavailable: {command}. "
                               "See docs/internal-rules/server-cpu-profiling.md for setup.")
    groups = sorted(regression.inventory(), key=lambda group: group["id"])
    priorities = scheduling.read_priorities(SUITE / "priorities.tsv", groups)
    for version in {group["version"] for group in groups}:
        catalog = args.sql_fun_home / "postgres" / version / "metadata.json"
        if json.loads(catalog.read_text())["server_version_num"] // 10000 != int(version):
            raise RuntimeError(f"Catalog version mismatch: {catalog}")
    return groups, priorities


def build(args, reports, metadata):
    env = dict(os.environ, SQL_FUN_HOME=str(args.sql_fun_home),
               CARGO_PROFILE_RELEASE_DEBUG="2", CARGO_PROFILE_RELEASE_STRIP="none",
               CARGO_PROFILE_RELEASE_OPT_LEVEL="3")
    # Encoded flags take precedence even when empty. Cargo splits RUSTFLAGS on
    # whitespace, not shell quoting. This checkout has no configured rustflags.
    flags = env["CARGO_ENCODED_RUSTFLAGS"].split("\x1f") if "CARGO_ENCODED_RUSTFLAGS" in env \
        else env.get("RUSTFLAGS", "").split()
    flags = [flag for flag in flags if flag] + ["-C", "force-frame-pointers=yes"]
    env["CARGO_ENCODED_RUSTFLAGS"] = "\x1f".join(flags)
    command = ["cargo", "build", "--locked", "--release", "-p", "sql-fun-cli", "-p", "sql-fun-server"]
    if args.features == "all":
        command.append("--all-features")
    metadata["build"] = dict(command=command, debug="2", strip="none", opt_level="3", profile="release",
                             rustflags=flags, rustc=subprocess.check_output(["rustc", "-Vv"], text=True))
    target = Path(json.loads(subprocess.check_output(
        ["cargo", "metadata", "--locked", "--no-deps", "--format-version=1"], cwd=ROOT, env=env))["target_directory"])
    with (reports / "build.log").open("w") as log:
        run(command, log, cwd=ROOT, env=env, stderr=subprocess.STDOUT)
    binaries = (target / "release/sql-fun", target / "release/sql-fun-server")
    metadata["binary_sha256"] = {str(path): digest(path) for path in binaries}
    return binaries


def controls_child(env, runtime, reports, versions):
    os.setsid()
    regression.run_controls(env, runtime, reports, versions)


def controls(env, runtime, reports, versions):
    # The unchanged control helper uses subprocess.run. Give its CLI children
    # their own group so interruption cannot leave an orphan control process.
    process = multiprocessing.get_context("fork").Process(
        target=controls_child, args=(env, runtime, reports, versions))
    process.start()
    try:
        while process.is_alive():
            process.join(0.25)
        if process.exitcode:
            raise RuntimeError(f"Regression controls failed: {process.exitcode}; inspect controls.log")
    finally:
        if process.is_alive():
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                process.kill()
            process.join()
        process.close()


def workload(groups, priorities, reports, runtime, env, server, summary):
    """Keep coordination aligned with run_suite; all SQL execution is reused."""
    workers = regression.run_workers(regression.ordered_tasks(groups, priorities), 2,
                                     env, runtime, reports, server)
    summary["worker_results"] = workers
    summary.update(regression.aggregate(groups, workers, reports))
    claims = [group for worker in workers for group in worker["claims"]]
    if collections.Counter(claims) != collections.Counter(group["id"] for group in groups):
        summary["problems"].append("Claimed group inventory mismatch")
    revisit = [group for group in groups if group["fixture"] in (
        "schema-domains-order-forward", "schema-domains-order-reverse",
        "project-roles", "project-roles-empty", "project-roles-omitted")]
    revisits = reports / "isolation-revisit"
    revisits.mkdir()
    results = regression.run_workers(regression.ordered_tasks(
        revisit, {group["id"]: priorities[group["id"]] for group in revisit}),
        2, env, runtime, revisits, server)
    isolation = regression.aggregate(revisit, results, revisits)
    write_json(revisits / "summary.json", isolation)
    summary["isolation_revisit"] = isolation
    summary["problems"].extend(isolation["problems"])
    controls(env, runtime, reports, sorted({group["version"] for group in groups}))
    server.verify()
    summary["ok"] = not summary["problems"]
    return summary


def capture(args, tools, binaries, groups, priorities, reports, metadata):
    suite_reports = reports / "suite"
    suite_reports.mkdir()
    summary = dict(workers=2, problems=[], ok=False)
    metadata["suite"] = summary
    with tempfile.TemporaryDirectory(prefix="sql-fun-regressions.", dir="/tmp") as name:
        runtime = Path(name)
        env = {key: value for key, value in os.environ.items() if key not in regression.CLEARED}
        cli, binary = binaries
        env.update(USER=runtime.name, SQL_FUN_HOME=str(args.sql_fun_home),
                   SQL_FUN_BIN=str(cli), SQL_FUN_TEST_SERVER_BIN=str(binary),
                   SQL_FUN_SERVER=str(binary), SQL_FUN_SERVER_COMMAND="exit 97", LC_ALL="C",
                   SQL_FUN_SERVER_IDLE_TIMEOUT=str(IDLE_TIMEOUT))
        metadata["namespace"] = runtime.name
        server = ProfiledServer(tools, binary, runtime, reports, env, metadata)
        began = time.monotonic()
        try:
            server.start()
            try:
                workload(groups, priorities, suite_reports, runtime, env, server, summary)
            except (Exception, KeyboardInterrupt) as error:
                summary["problems"].append(str(error) or "Interrupted")
                summary["ok"] = False
                if isinstance(error, KeyboardInterrupt):
                    raise
            finally:
                metadata["seconds"]["workload_and_startup"] = time.monotonic() - began
            server.finish()
        finally:
            # An interrupt/startup/finalization failure must retain the workload
            # result independently of whether perf can produce readable data.
            write_json(suite_reports / "summary.json", summary)
            server.close()


def measurement(args, reports, metadata, scratch):
    groups, priorities = prerequisites(args)
    metadata["expected_groups"] = len(groups)
    metadata["expected_cases"] = sum(group["count"] for group in groups)
    write_json(reports / "inventory.json", groups)
    shutil.copyfile(SUITE / "priorities.tsv", reports / "priorities.tsv")
    metadata["catalog_metadata_sha256"] = {
        version: digest(args.sql_fun_home / "postgres" / version / "metadata.json")
        for version in sorted({group["version"] for group in groups})}
    began = time.monotonic()
    root, flamegraph = provision(scratch, reports, args.mode)
    metadata["seconds"]["provision"] = time.monotonic() - began
    tools = Tools(args, metadata, root, flamegraph)
    began = time.monotonic()
    try:
        tools.check(reports)
    except Exception as error:
        raise RuntimeError(f"Profiling preflight failed: {error}. Inspect smoke/ and verify perf tools, "
                           "kernel.perf_event_paranoid and container perf_event_open permissions; "
                           "see docs/internal-rules/server-cpu-profiling.md.") from error
    metadata["seconds"]["smoke"] = time.monotonic() - began
    began = time.monotonic()
    binaries = build(args, reports, metadata)
    metadata["seconds"]["build"] = time.monotonic() - began
    try:
        capture(args, tools, binaries, groups, priorities, reports, metadata)
    finally:
        # Best-effort extraction also preserves useful evidence from failed
        # workloads/captures. The independent outcome gates below still fail.
        began = time.monotonic()
        try:
            tools.extract(reports)
            metadata["extraction_complete"] = True
        except (Exception, KeyboardInterrupt) as error:
            metadata["extraction_error"] = str(error) or "Interrupted"
        metadata["seconds"]["extraction"] = time.monotonic() - began
    if not metadata["suite"]["ok"]:
        raise RuntimeError("SQL regression workload failed; inspect suite/summary.json")
    if not metadata["capture_complete"] or not metadata["extraction_complete"]:
        raise RuntimeError("Capture or report extraction is incomplete; inspect run.json and logs")


def publish_latest(output, reports, mode):
    published = output / mode
    published.mkdir(exist_ok=True)
    names = ("flamegraph.svg", "run.json") if mode == "record" else ("stat.txt", "run.json")
    # Keep the report and metadata on one atomically replaced directory link,
    # following the heap profiler's publication approach, independently per mode.
    for name in names:
        path = published / name
        target = Path("latest") / name
        if path.is_symlink():
            if path.readlink() != target:
                raise RuntimeError(f"Unexpected stable output link: {path}")
        elif path.exists():
            raise RuntimeError(f"Stable output path already exists: {path}")
    for name in names:
        path = published / name
        if not path.is_symlink():
            path.symlink_to(Path("latest") / name)
    pending = reports / ".latest"
    pending.symlink_to(Path("..") / reports.name, target_is_directory=True)
    pending.replace(published / "latest")


def finish_run(args, reports, metadata, status):
    metadata.update(ok=status == 0, finished=datetime.now(timezone.utc).isoformat(),
                    persistent_bytes=sum(path.stat().st_size for path in reports.rglob("*") if path.is_file()))
    write_json(reports / "run.json", metadata)
    if status == 0:
        try:
            publish_latest(args.output, reports, args.mode)
        except Exception as error:
            status = 1
            metadata.update(ok=False, error=str(error))
            write_json(reports / "run.json", metadata)
    if status == 0:
        name = "flamegraph.svg" if args.mode == "record" else "stat.txt"
        print(f"SUCCESS: {args.mode} workload, capture and report complete.", flush=True)
        print(f"Latest report: {args.output / args.mode / name}\n"
              f"Latest metadata: {args.output / args.mode / 'run.json'}", flush=True)
    else:
        print(f"FAILURE: {metadata['error']}", file=sys.stderr, flush=True)
    print(f"Reports: {reports}", flush=True)
    return status


def main(argv=None):
    args = arguments(argv)
    os.chdir(ROOT)
    args.output.mkdir(parents=True, exist_ok=True)
    reports = Path(tempfile.mkdtemp(prefix="run-", dir=args.output))
    metadata = dict(mode=args.mode, features=args.features, sql_fun_home=str(args.sql_fun_home),
                    started=datetime.now(timezone.utc).isoformat(), ok=False,
                    capture_complete=False, extraction_complete=False, seconds={})
    print(f"Reports: {reports}", flush=True)
    status = 1
    try:
        metadata.update(revision=subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(),
                        dirty=bool(subprocess.check_output(["git", "status", "--porcelain"], text=True)),
                        kernel=os.uname().release,
                        perf_event_paranoid=Path("/proc/sys/kernel/perf_event_paranoid").read_text().strip())
        with tempfile.TemporaryDirectory(prefix="cpu-tools-", dir=args.output) as scratch:
            measurement(args, reports, metadata, Path(scratch))
        status = 0
    except (Exception, KeyboardInterrupt) as error:
        metadata["error"] = str(error) or "Interrupted"
        if isinstance(error, KeyboardInterrupt):
            status = 130
    finally:
        status = finish_run(args, reports, metadata, status)
    return status


if __name__ == "__main__":
    signal.signal(signal.SIGTERM, regression.interrupted)
    raise SystemExit(main())
