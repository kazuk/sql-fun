"""Profile the complete CI SQL workload locally, without changing CI execution."""
import argparse
import collections
from datetime import datetime, timezone
import json
import multiprocessing
import os
from pathlib import Path
import shlex
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

from server import ProfiledServer
from tools import digest, provision, run


def size(directory):
    return sum(path.stat().st_size for path in directory.rglob("*") if path.is_file())


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n")


def arguments(argv):
    parser = argparse.ArgumentParser(prog="profile-server-heap", description=__doc__,
                                     formatter_class=argparse.ArgumentDefaultsHelpFormatter)
    parser.add_argument("--features", default="default", choices=("default", "all"),
                        help="Feature variant to profile")
    parser.add_argument("--sql-fun-home", type=Path, default=ROOT / "sql-fun-home",
                        help="Builtin catalog installation")
    parser.add_argument("--output", type=Path, default=ROOT / "target/heap-reports",
                        help="Persistent report parent directory")
    parser.add_argument("--raw-dir", type=Path, default=ROOT / "target/heap-scratch",
                        help="Temporary profiling data parent directory")
    parser.add_argument("--keep-raw", action="store_true")
    args = parser.parse_args(argv)
    for key in ("sql_fun_home", "output", "raw_dir"):
        path = getattr(args, key)
        if not path.is_absolute() or any(char in str(path) for char in '\n\r"\\%'):
            parser.error(f"--{key.replace('_', '-')} must be an absolute path without quotes, backslashes, percent signs or newlines")
    os_release = Path("/etc/os-release").read_text()
    if ('ID=ubuntu\n' not in os_release or 'VERSION_ID="24.04"' not in os_release
            or os.uname().machine != "x86_64"):
        parser.error("Automatic profiling tool provisioning currently supports Ubuntu 24.04 amd64 only")
    for command in ("cargo", "git", "apt-get", "dpkg-deb", "dpkg-architecture", "bash", "jq", "cc", "gzip"):
        if not shutil.which(command):
            parser.error(f"Required profiling command is unavailable: {command}")
    groups = sorted(regression.inventory(), key=lambda group: group["id"])
    priorities = scheduling.read_priorities(SUITE / "priorities.tsv", groups)
    for version in {group["version"] for group in groups}:
        catalog = args.sql_fun_home / "postgres" / version / "metadata.json"
        if json.loads(catalog.read_text())["server_version_num"] // 10000 != int(version):
            parser.error(f"Catalog version mismatch: {catalog}")
    target = Path(json.loads(subprocess.check_output(
        ["cargo", "metadata", "--locked", "--no-deps", "--format-version=1"], cwd=ROOT))["target_directory"])
    return args, groups, priorities, target


def build(args, target, reports, metadata):
    env = dict(os.environ, SQL_FUN_HOME=str(args.sql_fun_home),
               CARGO_PROFILE_RELEASE_DEBUG="2", CARGO_PROFILE_RELEASE_STRIP="none",
               CARGO_PROFILE_RELEASE_OPT_LEVEL="3")
    command = ["cargo", "build", "--locked", "--release", "-p", "sql-fun-cli", "-p", "sql-fun-server"]
    if args.features == "all":
        command.append("--all-features")
    metadata["build"] = dict(command=command, debug="2", strip="none", opt_level="3", profile="release",
                             rustflags=env.get("CARGO_ENCODED_RUSTFLAGS", env.get("RUSTFLAGS", "")),
                             rustc=subprocess.check_output(["rustc", "-Vv"], text=True))
    with (reports / "build.log").open("w") as log:
        run(command, log, cwd=ROOT, env=env)
    return target / "release/sql-fun", target / "release/sql-fun-server"


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


def capture(args, tools, cli, binary, groups, priorities, raw, reports, metadata):
    suite_reports = reports / "suite"
    suite_reports.mkdir()
    with tempfile.TemporaryDirectory(prefix="sql-fun-regressions.", dir="/tmp") as name:
        runtime = Path(name)
        env = {key: value for key, value in os.environ.items() if key not in regression.CLEARED}
        for key in ("LD_PRELOAD", "LD_LIBRARY_PATH", "DUMP_HEAPTRACK_OUTPUT"):
            env.pop(key, None)
        env.update(USER=runtime.name, SQL_FUN_HOME=str(args.sql_fun_home),
                   SQL_FUN_BIN=str(cli), SQL_FUN_TEST_SERVER_BIN=str(binary),
                   SQL_FUN_SERVER=str(binary), SQL_FUN_SERVER_COMMAND="exit 97", LC_ALL="C",
                   SQL_FUN_SERVER_IDLE_TIMEOUT="180")
        # Only the profiler and its children receive private runtime libraries.
        server = ProfiledServer(tools, binary, runtime, raw, reports, env)
        began = time.monotonic()
        try:
            server.start()
            metadata["server_pid"] = server.server_pid
            summary = dict(workers=2, problems=[])
            try:
                workload(groups, priorities, suite_reports, runtime, env, server, summary)
            except Exception as error:
                summary["problems"].append(str(error))
                summary["ok"] = False
            write_json(suite_reports / "summary.json", summary)
            metadata["suite"] = summary
            metadata["seconds"]["workload_and_startup"] = time.monotonic() - began
            began = time.monotonic()
            profile = server.finish()
            metadata["seconds"]["finalization"] = time.monotonic() - began
            metadata["capture_complete"] = True
            return profile
        finally:
            server.close()


def recovery(raw, tools, profile, reports):
    # Executable recovery command survives cleanup of the coordinator process.
    command = ["env", "LD_LIBRARY_PATH=" + tools.env["LD_LIBRARY_PATH"],
               str(tools.printer), str(profile), "--merge-backtraces=false", "--print-leaks=true",
               "--disable-builtin-suppressions", "--disable-embedded-suppressions"]
    demangler = ["env", "LD_LIBRARY_PATH=" + tools.env["LD_LIBRARY_PATH"],
                 str(tools.demangler), "--format=rust"]
    pipeline = shlex.join(command) + " | " + shlex.join(demangler)
    (reports / "recovery.txt").write_text(
        "Raw/tools retained at: " + str(raw) + "\nInspect or retry native extraction:\n" +
        shlex.join(["bash", "-o", "pipefail", "-c", pipeline]) +
        "\nAfter inspection, manually remove only this run's directory:\n" +
        shlex.join(["rm", "-r", "--", str(raw)]) + "\n")


def main(argv=None):
    args, groups, priorities, target = arguments(argv)
    os.chdir(ROOT)
    args.output.mkdir(parents=True, exist_ok=True)
    args.raw_dir.mkdir(parents=True, exist_ok=True)
    reports = Path(tempfile.mkdtemp(prefix="run-", dir=args.output))
    raw = Path(tempfile.mkdtemp(prefix="heap-profile-", dir=args.raw_dir))
    metadata = dict(revision=subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(),
                    dirty=bool(subprocess.check_output(["git", "status", "--porcelain"], text=True)),
                    started=datetime.now(timezone.utc).isoformat(), features=args.features,
                    sql_fun_home=str(args.sql_fun_home), raw_directory=str(raw),
                    raw_retained=True, capture_complete=False, extraction_complete=False, seconds={})
    metadata["expected_groups"] = len(groups)
    metadata["expected_cases"] = sum(group["count"] for group in groups)
    write_json(reports / "inventory.json", groups)
    shutil.copyfile(SUITE / "priorities.tsv", reports / "priorities.tsv")
    metadata["catalog_metadata_sha256"] = {
        version: digest(args.sql_fun_home / "postgres" / version / "metadata.json")
        for version in sorted({group["version"] for group in groups})}
    print(f"Reports: {reports}\nRaw/tools: {raw}", flush=True)
    status = 1
    try:
        began = time.monotonic()
        tools = provision(raw, reports)
        tools.check(raw, reports)
        metadata["seconds"]["provision_and_smoke"] = time.monotonic() - began
        began = time.monotonic()
        cli, binary = build(args, target, reports, metadata)
        metadata["seconds"]["build"] = time.monotonic() - began
        metadata["binary_sha256"] = {str(path): digest(path) for path in (cli, binary)}
        profile = capture(args, tools, cli, binary, groups, priorities, raw, reports, metadata)
        recovery(raw, tools, profile, reports)
        metadata["raw_profile_bytes"] = profile.stat().st_size
        metadata["temporary_bytes"] = size(raw)
        began = time.monotonic()
        metadata["extraction_command"] = tools.extract(profile, reports / "report.txt")
        metadata["seconds"]["extraction"] = time.monotonic() - began
        metadata["extraction_complete"] = True
        metadata["report_bytes"] = (reports / "report.txt").stat().st_size
        # Publish durable outcome information before deleting the source trace.
        write_json(reports / "run.json", metadata)
        if not args.keep_raw:
            shutil.rmtree(raw)
            metadata["raw_retained"] = False
            (reports / "recovery.txt").unlink()
        status = 0 if metadata["suite"]["ok"] else 1
    except (Exception, KeyboardInterrupt) as error:
        metadata["error"] = str(error) or "Interrupted"
        print(f"Profiling failed: {metadata['error']}", file=sys.stderr, flush=True)
        if isinstance(error, KeyboardInterrupt):
            status = 130
    finally:
        metadata["finished"] = datetime.now(timezone.utc).isoformat()
        metadata["persistent_bytes"] = size(reports)
        write_json(reports / "run.json", metadata)
        print(f"Reports: {reports}", flush=True)
        if metadata["raw_retained"]:
            print(f"Raw/tools retained: {raw}; see recovery.txt if extraction was reached.", flush=True)
            if (reports / "recovery.txt").exists():
                print((reports / "recovery.txt").read_text(), flush=True)
    return status


if __name__ == "__main__":
    signal.signal(signal.SIGTERM, regression.interrupted)
    raise SystemExit(main())
