# Local server CPU profiling

Use `profile-server-cpu` to locate expensive server call stacks or collect CPU
statistics across the complete SQLAST CLI regression workload. This optional
contributor command is separate from normal verification and CI. It preserves
the fixture manifest, expectations, committed priorities, shared server, two
workers, isolation revisits and negative controls. SQL is analyzed using the
selected installed builtin catalogs; it is not executed against a live database.

## Prerequisites

Use the [contributor environment](../../CONTRIBUTING.md#getting-started).
The initial supported environment is local Linux development on Ubuntu 24.04.
Both modes require Cargo/Rust, uv with Python 3.12, Bash, jq, a C compiler for
the short capability check, Git and a working Linux `perf`. These profiling
prerequisites are not required for normal development or installed sql-fun use.

Install perf appropriate to the **running kernel**. On Ubuntu, for example:

```sh
sudo apt-get install linux-tools-common "linux-tools-$(uname -r)"
perf version
```

A container uses its host kernel: its distribution's default tools package
might not match that kernel, and the exact package might not be available in
its configured archives. Make a compatible perf executable available on `PATH`;
installing a generic package alone does not prove capture works.

`record` additionally requires Perl, GNU binutils `c++filt` with Rust v0
symbol support, and an explicitly selected checkout of
[FlameGraph](https://github.com/brendangregg/FlameGraph):

```sh
sudo apt-get install perl binutils
git clone https://github.com/brendangregg/FlameGraph.git /absolute/path/to/FlameGraph
```

The checkout must contain `stackcollapse-perf.pl` and `flamegraph.pl`.
Its revision and script hashes are recorded. The command does not download tools,
install packages, change permissions or alter kernel settings. `stat` does not
require Perl, binutils or a FlameGraph checkout.

## Choose one measurement

Select the feature variant and catalog installation explicitly. For a FlameGraph:

```sh
profile-server-cpu --mode record --features default \
  --sql-fun-home /absolute/path/to/sql-fun-home \
  --flamegraph /absolute/path/to/FlameGraph
```

Omitting `--mode` selects `record`. For aggregate statistics:

```sh
profile-server-cpu --mode stat --features default \
  --sql-fun-home /absolute/path/to/sql-fun-home
```

Each invocation executes the complete workload **once**, using a fresh owned
server and discovery namespace. To obtain both reports, invoke the command twice,
selecting each mode separately. There is no automatic second pass or retry.
Use `--features all` for the all-features variant. Catalogs for every version in
the committed manifest must be installed at the selected location.

Reports are stored in a fresh `target/cpu-reports/run-*` directory relative to
the actual repository root, regardless of the caller's directory. Override the
parent with `--output /absolute/path/to/reports`. All path arguments must be
absolute. Outputs from different invocations are independent measurements.

A small mode-specific capture check runs before the expensive build. The command
then builds the selected CLI and server with locked release dependencies,
optimization level 3, full debug information, no stripping and frame pointers.
Settings apply only to this invocation; normal Cargo profiles are unchanged.
Cargo's target directory and effective encoded/plain environment Rust flags
are respected, with frame pointers enabled for the profiling build.

## Results and interpretation

The command prints `SUCCESS` and returns zero only when the regression workload,
capture and selected report all succeed. Failures print `FAILURE`, return nonzero
and retain available evidence. A readable profile does not turn a failed suite
or an incomplete capture into a successful run. Expected negative-control
failures are checked by the existing suite and are not workload failures.

Inspect the printed run directory:

- `run.json`: overall result, selected mode/features, revision and dirty state,
  kernel, tool/build settings, binary/catalog hashes, server identity, workload,
  capture and report outcomes, timings and report storage.
- `build.log`, `server.log`, `smoke/`: build output, server/perf diagnostics and
  capability-check evidence. Perf's lost-sample warnings remain in its log.
- `suite/`: unchanged worker logs, JUnit reports, aggregation, claims, isolation
  revisits and negative controls.
- `record`: `flamegraph.svg`, `stacks.folded`, `stacks.txt`, `perf.script`,
  `extraction.log` and raw `perf.data`. Open the SVG in a browser. Raw data and
  intermediate text are retained for investigation and re-extraction.
- `stat`: native `stat.txt`, including supported task-clock, context-switch and
  page-fault counters and perf's available user/system CPU-time summaries.
  Unsupported optional counters are recorded in `run.json`; absent statistics
  are unavailable, not zero. Selected counters that were not actually measured
  fail the run.

`record` uses software `cpu-clock:u` sampling at 99 Hz with frame-pointer call
graphs, including server threads. The FlameGraph describes **sampled user-space
on-CPU activity**, not off-CPU waits, kernel stacks or CLI execution. Native
libraries without frame pointers and optimized/inlined Rust frames can produce
incomplete stacks. Inspect lost-sample diagnostics when interpreting a capture;
this is a profiler, not a latency benchmark.

`stat` wraps the server directly and does not count a nested recording profiler.
Keep its task-clock and CPU-time statistics separate from regression wall-clock
timings. Reports from two invocations describe different executions, not the same
samples. Neither mode attaches to an existing developer server.

The profiling-only server uses the existing 10-second idle shutdown setting.
After the workload, the command waits up to 70 seconds for natural shutdown and
perf finalization. Failure/interruption triggers bounded cleanup, allowing perf
to drain data before forced termination when possible. Forced or otherwise
incomplete captures never count as successful results.

## Permission failures and storage

A working `perf version` does not establish permission to profile. The smoke
check exercises the selected mode and, for `record`, stack extraction and
symbolization. On failure inspect `smoke/smoke.log`, the smoke stat reports and
`run.json`. Check the host's `kernel.perf_event_paranoid`, permissions such as
`CAP_PERFMON`, and container seccomp restrictions on `perf_event_open`. A host
administrator must choose any necessary permission changes. A changed host
sysctl alone does not establish that capture works inside a container.

Reserve at least 20 GiB free before building; 40–60 GiB is a preliminary allowance
for symbol-rich builds, ordinary verification and retained captures. Actual use
depends on the build cache and workload. Large `perf.script` files and raw data
are retained, including on failure. Record build, workload, finalization and
extraction times and storage from the initial measurement before considering
any recurring execution. Remove only your own finished run directory when its
evidence is no longer needed; do not delete active runs or other tasks' builds.

The implementation lives under `.local/sbin/cpu-profile/`. Its profiling-only
workload coordination must stay aligned with
`tests/sqlast-regressions/parallel_runner.py` when that workload changes.
