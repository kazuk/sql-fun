#!/usr/bin/env python3
"""Opt-in shared-server regression experiment; standard library only."""
import argparse
import collections
import concurrent.futures
import copy
import json
import math
import os
from pathlib import Path
import queue
import re
import select
import signal
import shutil
import statistics
import subprocess
import tempfile
import threading
import time
import xml.etree.ElementTree as ET

SUITE = Path(__file__).resolve().parent
CLEARED = "SQL_FUN_SERVER_COMMAND SQL_FUN_METADATA_FILE SQL_FUN_DATASOURCE RUST_LOG BASH_ENV SQL_FUN_SCHEMA SQL_FUN_SCHEMA_FILE SQL_FUN_DIALECT SQL_FUN_CTE_CATALOG_DIR SQL_FUN_METADATA_FORMAT SQL_FUN_NO_BUILTIN SQL_FUN_HIGHLIGHTER_THEME SQL_FUN_MODE SQL_FUN_PG_SEARCH_PATH SQL_FUN_POSTGRES_VERSION SQL_FUN_EXTENSIONS SQL_FUN_BUILTIN_SCHEMA SQL_FUN_EXTENSIONS_DIR SQL_FUN_LOG_FILE SQL_FUN_OUTPUT_FORMAT".split()


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n")


def inventory(suite=SUITE):
    groups, fixtures = [], set()
    for line in (suite / "manifest.tsv").read_text().splitlines():
        if not line or line.startswith("#"):
            continue
        fixture, level, schema, count, versions = line.split("\t")
        if not re.fullmatch(r"[a-z0-9_-]+", fixture) or fixture in fixtures:
            raise ValueError(f"Invalid/duplicate fixture: {fixture}")
        fixtures.add(fixture)
        if level not in ("builtin", "default", "extensions", "project"):
            raise ValueError(f"Invalid schema level: {level}")
        if schema != "-" and (Path(schema).name != schema or not (suite / schema).is_file()):
            raise ValueError(f"Invalid schema file: {schema}")
        cases = json.loads((suite / f"{fixture}.json").read_text())
        names = [case["name"] for case in cases]
        if int(count) <= 0 or len(cases) != int(count) or any(not isinstance(n, str) or not n for n in names) or len(set(names)) != len(names):
            raise ValueError(f"Invalid cases/count/names: {fixture}")
        for version in versions.split(","):
            if version not in ("17", "18"):
                raise ValueError(f"Invalid version: {version}")
            groups.append(dict(id=f"{fixture}-{version}", fixture=fixture, level=level,
                               schema=schema, count=int(count), version=version,
                               names=[f"{suite / (fixture + '.json')}::{n}" for n in names]))
    if not groups or len({g["id"] for g in groups}) != len(groups):
        raise ValueError("Empty or duplicate group inventory")
    if {p.stem for p in suite.glob("*.json")} != fixtures:
        raise ValueError("Manifest/fixture registration mismatch")
    return groups


def ordered_tasks(groups, estimates):
    if estimates and set(estimates) != {g["id"] for g in groups}:
        raise ValueError("Duration estimates do not cover the exact inventory")
    if any(not isinstance(v, (float, int)) or not math.isfinite(v) or v < 0 for v in estimates.values()):
        raise ValueError("Invalid duration estimate")
    return sorted(groups, key=lambda g: -estimates.get(g["id"], 0))


def aggregate(groups, workers, directory):
    """Treat inventory and process exits as independent evidence, not XML counters."""
    root = ET.Element("testsuites")
    problems, seen, completed = [], [], []
    totals = dict(tests=0, failures=0, errors=0, skipped=0)
    expected = {g["id"]: g for g in groups}
    for worker in workers:
        worker_dir = directory / f"worker-{worker['worker']}"
        actual_reports = {p.stem for p in worker_dir.glob("*.xml")}
        try:
            if json.loads((worker_dir / "completion.json").read_text()) != worker:
                raise ValueError("Completion record differs from coordinator evidence")
            if actual_reports != {c["id"] for c in worker["completions"]}:
                raise ValueError("Missing/unexpected worker reports")
        except (OSError, ValueError) as error:
            problems.append(f"Worker {worker['worker']}: {error}")
        if worker.get("exit") != 0:
            problems.append(f"Worker {worker['worker']} exit: {worker.get('exit')}")
        if worker.get("problem"):
            problems.append(worker["problem"])
        completed.extend(c["id"] for c in worker["completions"])
        for completion in worker["completions"]:
            if completion.get("status") != 0:
                problems.append(f"Group exit: {completion['id']}: {completion.get('status')}")
        # Retain valid available XML even if a worker died before acknowledging
        # completion. Missing acknowledgments still fail the independent checks.
        for ident in sorted(actual_reports | {c["id"] for c in worker["completions"]}):
            if ident not in expected:
                problems.append(f"Unexpected group: {ident}")
                continue
            group = expected[ident]
            try:
                document = ET.parse(directory / f"worker-{worker['worker']}" / f"{ident}.xml").getroot()
                suites = document.findall("testsuite")
                if document.tag != "testsuites" or len(suites) != 1:
                    raise ValueError("Expected one testsuite")
                suite = copy.deepcopy(suites[0])
                cases = suite.findall("testcase")
                names = [c.attrib["name"] for c in cases]
                if collections.Counter(names) != collections.Counter(group["names"]):
                    raise ValueError("Case identity mismatch")
                counts = dict(tests=len(cases), failures=0, errors=0, skipped=0)
                for case in cases:
                    outcomes = [tag for tag in ("failure", "error", "skipped") if case.find(tag) is not None]
                    if len(outcomes) > 1:
                        raise ValueError("Conflicting testcase outcomes")
                    if outcomes:
                        counts[{"failure": "failures", "error": "errors", "skipped": "skipped"}[outcomes[0]]] += 1
                for key, value in counts.items():
                    try:
                        declared = int(suite.get(key, "0"))
                    except ValueError:
                        declared = None
                    if declared != value:
                        problems.append(f"{ident}: declared {key} mismatch")
                    totals[key] += value
                    suite.set(key, str(value))
                suite.set("name", ident)
                root.append(suite)
                seen.append(ident)
            except (OSError, ET.ParseError, ValueError, KeyError) as error:
                problems.append(f"{ident}: {error}")
    wanted = collections.Counter(expected.keys())
    if collections.Counter(completed) != wanted:
        problems.append("Completed group inventory mismatch (missing/duplicate/unexpected)")
    if collections.Counter(seen) != wanted:
        problems.append("Reported group inventory mismatch (missing/duplicate/unexpected)")
    if totals["failures"] or totals["errors"]:
        problems.append("Regression failures/errors")
    for key, value in totals.items():
        root.set(key, str(value))
    ET.indent(root)
    ET.ElementTree(root).write(directory / "combined.xml", encoding="utf-8", xml_declaration=True)
    return dict(outcomes=totals, problems=problems)


def stop(process):
    # A shell worker owns its CLI children in a separate process group. Never
    # signal a previously reaped group again: its numeric ID may be reused.
    if getattr(process, "_experiment_stopped", False):
        return
    process._experiment_stopped = True
    try:
        os.killpg(process.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    process.wait()


class Server:
    def __init__(self, env, runtime, reports):
        self.env, self.runtime, self.reports = env, runtime, reports
        self.process = None
        self.samples = []
        self.done = threading.Event()
        self.problem = None

    def verify(self):
        if self.process.poll() is not None:
            raise RuntimeError(f"Owned server exited: {self.process.returncode}")
        if (self.runtime / "sql-fun-server.pid").read_bytes() != self.registration:
            raise RuntimeError("Owned server registration changed")

    def sample(self):
        while not self.done.is_set():
            try:
                self.verify()
                status = Path(f"/proc/{self.process.pid}/status").read_text()
                values = {k: v.strip() for k, v in (line.split(":", 1) for line in status.splitlines())}
                # /proc stat fields after comm: state=3, utime=14, stime=15.
                fields = Path(f"/proc/{self.process.pid}/stat").read_text().rsplit(")", 1)[1].split()
                self.samples.append(dict(seconds=time.monotonic() - self.started,
                                         cpu_seconds=(int(fields[11]) + int(fields[12])) / os.sysconf("SC_CLK_TCK"),
                                         rss=values.get("VmRSS"), peak_rss=values.get("VmHWM"), threads=values.get("Threads")))
            except (OSError, ValueError, RuntimeError) as error:
                self.problem = str(error)
                return
            self.done.wait(0.25)

    def __enter__(self):
        self.started = time.monotonic()
        self.log = (self.reports / "server.log").open("w")
        self.process = subprocess.Popen([self.env["SQL_FUN_TEST_SERVER_BIN"]], env=self.env,
                                        stdin=subprocess.DEVNULL, stdout=self.log, stderr=subprocess.STDOUT,
                                        start_new_session=True)
        try:
            registration = self.runtime / "sql-fun-server.pid"
            for _ in range(200):
                if self.process.poll() is not None:
                    raise RuntimeError("Server exited before readiness")
                if registration.exists() and registration.stat().st_size:
                    self.registration = registration.read_bytes()
                    try:
                        registered_pid = json.loads(self.registration)["pid"]
                    except (json.JSONDecodeError, KeyError):
                        time.sleep(0.05)
                        continue
                    if registered_pid != self.process.pid:
                        raise RuntimeError("Registration is not the owned server")
                    self.verify()
                    self.thread = threading.Thread(target=self.sample)
                    self.thread.start()
                    return self
                time.sleep(0.05)
            raise RuntimeError("Server readiness timed out")
        except BaseException:
            stop(self.process)
            self.log.close()
            raise

    def __exit__(self, *_):
        self.done.set()
        self.thread.join()
        stop(self.process)
        self.log.close()
        write_json(self.reports / "server-resources.json", self.samples)


def run_workers(tasks, count, env, runtime, directory, server):
    pending = queue.Queue()
    for group in tasks:
        pending.put(group)
    abort = threading.Event()
    children, claims, lock = [], [], threading.Lock()

    def work(number):
        began = time.monotonic()
        reports = directory / f"worker-{number}"
        reports.mkdir()
        result = dict(worker=number, completions=[], claims=[], exit=None)
        with (reports / "stderr.log").open("w") as errors:
            process = subprocess.Popen(["bash", str(SUITE / "worker.sh"), str(runtime / f"worker-{number}"), str(reports)],
                                       env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=errors,
                                       text=True, bufsize=1, start_new_session=True)
            with lock:
                children.append(process)
            try:
                while not abort.is_set():
                    server.verify()
                    with lock:
                        try:
                            group = pending.get_nowait()
                        except queue.Empty:
                            break
                        claims.append(dict(group=group["id"], worker=number, seconds=time.monotonic() - began))
                        result["claims"].append(group["id"])
                    process.stdin.write("\t".join(str(group[k]) for k in ("fixture", "level", "schema", "count", "version")) + "\n")
                    process.stdin.flush()
                    # Real CLI connection operations have their own bounded timeout.
                    deadline = time.monotonic() + 1800
                    while not select.select([process.stdout], [], [], 0.25)[0]:
                        server.verify()
                        if abort.is_set() or time.monotonic() >= deadline:
                            raise RuntimeError("Worker completion timed out or run aborted")
                    line = process.stdout.readline()
                    completion = json.loads(line)
                    if (completion["fixture"], completion["version"]) != (group["fixture"], group["version"]):
                        raise ValueError("Worker completion does not match claim")
                    completion["id"] = group["id"]
                    result["completions"].append(completion)
                    server.verify()
                process.stdin.close()
                result["exit"] = process.wait()
            except (OSError, ValueError, KeyError, RuntimeError) as error:
                result["problem"] = f"Worker {number}: {error}"
                abort.set()
            finally:
                stop(process)
                process.stdout.close()
                if not process.stdin.closed:
                    process.stdin.close()
                result["seconds"] = time.monotonic() - began
                write_json(reports / "completion.json", result)
        return result

    executor = concurrent.futures.ThreadPoolExecutor(max_workers=count)
    try:
        results = list(executor.map(work, range(count)))
    finally:
        abort.set()
        with lock:
            for child in children:
                stop(child)
        executor.shutdown(wait=True)
        write_json(directory / "claims.json", claims)
    return results


def run_controls(env, runtime, reports, versions):
    # The same Bash operation also serves the normal serial source/installed path.
    script = '''set -euo pipefail
suite=$1 runtime=$2 reports=$3
source "$suite/operations.sh"
declare -A checked_versions=()
shift 3
for version in "$@"; do checked_versions[$version]=1; done
fixture=pilot
run_controls
'''
    with (reports / "controls.log").open("w") as log:
        subprocess.run(["bash", "-c", script, "controls", str(SUITE), str(runtime), str(reports), *versions],
                       env=env, stdout=log, stderr=subprocess.STDOUT, check=True)


def trial(groups, count, estimates, reports, base_env):
    began = time.monotonic()
    reports.mkdir()
    summary = dict(workers=count, problems=[])
    with tempfile.TemporaryDirectory(prefix="sql-fun-experiment.", dir="/tmp") as temp:
        runtime = Path(temp)
        env = {k: v for k, v in base_env.items() if k not in CLEARED}
        env.update(USER=runtime.name, SQL_FUN_SERVER=base_env["SQL_FUN_TEST_SERVER_BIN"], LC_ALL="C")
        # Refuse client auto-relaunch if the owned server dies. No extra server is
        # allowed to make an apparently successful shared-server measurement.
        env["SQL_FUN_SERVER_COMMAND"] = "exit 97"
        try:
            with Server(env, runtime, reports) as server:
                summary["server_pid"] = server.process.pid
                summary["namespace"] = runtime.name
                positive_started = time.monotonic()
                workers = run_workers(ordered_tasks(groups, estimates), count, env, runtime, reports, server)
                summary["positive_seconds"] = time.monotonic() - positive_started
                summary["worker_results"] = workers
                summary.update(aggregate(groups, workers, reports))
                claims = [g for w in workers for g in w["claims"]]
                if collections.Counter(claims) != collections.Counter(g["id"] for g in groups):
                    summary["problems"].append("Claimed group inventory mismatch")
                # Revisit conflicting project declarations and role selections
                # after the full suite has exercised other cache contexts.
                revisit = [g for g in groups if g["fixture"] in (
                    "schema-domains-order-forward", "schema-domains-order-reverse",
                    "project-roles", "project-roles-empty", "project-roles-omitted")]
                revisit_reports = reports / "isolation-revisit"
                revisit_reports.mkdir()
                revisited = run_workers(ordered_tasks(revisit, {g["id"]: estimates.get(g["id"], 0) for g in revisit}),
                                        count, env, runtime, revisit_reports, server)
                isolation = aggregate(revisit, revisited, revisit_reports)
                write_json(revisit_reports / "summary.json", isolation)
                summary["isolation_revisit"] = isolation
                summary["problems"].extend(isolation["problems"])
                run_controls(env, runtime, reports, sorted({g["version"] for g in groups}))
                server.verify()
                if server.problem:
                    summary["problems"].append(server.problem)
        except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
            summary["problems"].append(str(error))
    summary["wall_seconds"] = time.monotonic() - began
    summary["ok"] = not summary["problems"]
    summary["command_seconds"] = sum(c["seconds"] for w in summary.get("worker_results", []) for c in w["completions"])
    if "positive_seconds" in summary and "outcomes" in summary:
        summary["cases_per_second"] = summary["outcomes"]["tests"] / summary["positive_seconds"]
    write_json(reports / "summary.json", summary)
    print(f"{reports.name}: workers={count} wall={summary['wall_seconds']:.3f}s ok={summary['ok']}", flush=True)
    return summary


def validate_environment(parser, groups):
    for executable in ("bash", "jq"):
        if not shutil.which(executable):
            parser.error(f"Required experiment tool is unavailable: {executable}")
    for key in ("SQL_FUN_HOME", "SQL_FUN_BIN", "SQL_FUN_TEST_SERVER_BIN"):
        if key not in os.environ or not Path(os.environ[key]).is_absolute():
            parser.error(f"{key} must explicitly select an absolute path")
    for key in ("SQL_FUN_BIN", "SQL_FUN_TEST_SERVER_BIN"):
        if not os.access(os.environ[key], os.X_OK):
            parser.error(f"Not executable: {os.environ[key]}")
    for version in {g["version"] for g in groups}:
        catalog = Path(os.environ["SQL_FUN_HOME"]) / "postgres" / version / "metadata.json"
        if json.loads(catalog.read_text())["server_version_num"] // 10000 != int(version):
            parser.error(f"Catalog version mismatch: {catalog}")


def record_environment(output, groups):
    write_json(output / "inventory.json", groups)
    write_json(output / "environment.json", dict(cpu_count=os.cpu_count(), affinity=sorted(os.sched_getaffinity(0)),
               uname=list(os.uname()), memory=Path('/proc/meminfo').read_text(),
               revision=subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=SUITE, text=True).strip(),
               runner=os.environ.get("RUNNER_NAME"), runner_environment=os.environ.get("RUNNER_ENVIRONMENT"),
               cgroup_limits={name: (Path('/sys/fs/cgroup') / name).read_text().strip()
                              for name in ('cpu.max', 'memory.max') if (Path('/sys/fs/cgroup') / name).is_file()},
               selections={k: os.environ[k] for k in ("SQL_FUN_HOME", "SQL_FUN_BIN", "SQL_FUN_TEST_SERVER_BIN")}))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--workers", required=True, nargs="+", type=int)
    parser.add_argument("--repetitions", required=True, type=int)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    if not args.workers or args.workers[0] != 1 or len(set(args.workers)) != len(args.workers) or any(n not in (1, 2, 4, 8) for n in args.workers) or args.repetitions < 1:
        parser.error("Select a unique worker sequence starting with 1, followed by 2/4/8, and positive repetitions")
    groups = inventory()
    validate_environment(parser, groups)
    args.output = args.output.resolve()
    args.output.mkdir(parents=True, exist_ok=False)
    record_environment(args.output, groups)
    # Calibration is separate so every compared configuration, including all
    # serial repeats, uses the same longest-estimated-duration-first queue.
    calibration = trial(groups, 1, {}, args.output / "estimate-baseline", os.environ)
    write_json(args.output / "calibration.json", calibration)
    if not calibration["ok"]:
        return 1
    estimates = {c["id"]: c["seconds"] for w in calibration["worker_results"] for c in w["completions"]}
    write_json(args.output / "estimates.json", dict(source="estimate-baseline", seconds=estimates))
    results = []
    for repetition in range(args.repetitions):
        order = args.workers[repetition % len(args.workers):] + args.workers[:repetition % len(args.workers)]
        for count in order:
            result = trial(groups, count, estimates, args.output / f"repeat-{repetition + 1}-workers-{count}", os.environ)
            results.append(result)
            write_json(args.output / "results.json", results)
            if not result["ok"]:
                return 1
    comparison = {}
    baseline = statistics.median(r["wall_seconds"] for r in results if r["workers"] == 1)
    positive_baseline = statistics.median(r["positive_seconds"] for r in results if r["workers"] == 1)
    for count in args.workers:
        samples = [r["wall_seconds"] for r in results if r["workers"] == count]
        positive = [r["positive_seconds"] for r in results if r["workers"] == count]
        comparison[count] = dict(samples=samples, median=statistics.median(samples), minimum=min(samples), maximum=max(samples),
                                 speedup=baseline / statistics.median(samples), positive_samples=positive,
                                 positive_median=statistics.median(positive), positive_speedup=positive_baseline / statistics.median(positive))
    write_json(args.output / "comparison.json", comparison)
    print(json.dumps(comparison, indent=2))
    return 0


def interrupted(signum, _frame):
    raise KeyboardInterrupt(f"Interrupted by signal {signum}")


if __name__ == "__main__":
    signal.signal(signal.SIGTERM, interrupted)
    raise SystemExit(main())
