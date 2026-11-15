# Local server CPU profiling

Use `profile-server-cpu` to locate expensive server call stacks or collect CPU
statistics across the complete SQLAST CLI regression workload. This optional
contributor command is separate from normal verification and CI. It preserves
the fixture manifest, expectations, committed priorities, shared server, two
workers, isolation revisits and negative controls. SQL is analyzed using the
selected installed builtin catalogs; it is not executed against a live database.

## Prerequisites

Use the [contributor environment](../../CONTRIBUTING.md#getting-started).
Automatic tool preparation supports Ubuntu 24.04 amd64. Both modes require
Cargo/Rust, uv with Python 3.12, Bash, jq, a C compiler for the short capability
check, Git, APT and dpkg tools from that environment.

The command downloads Ubuntu packages into a private temporary APT tree and
extracts perf and its runtime dependencies without installing system packages or
changing the host's APT configuration/cache. Record mode also prepares binutils,
Perl and the project's pinned FlameGraph revision. Stat does not prepare those
record-only tools. Network access to Ubuntu package archives and GitHub is
required. Temporary tools are removed when the invocation finishes; reports
retain package identities/hashes, preparation logs and time, and FlameGraph
revision/script hashes.

The actual packaged perf executable is used instead of Ubuntu's kernel-selecting
wrapper. A container shares its host kernel; compatibility and capture permission
are verified by real mode-specific smoke measurements before the expensive build.
Tool preparation does not change kernel settings, capabilities or seccomp policy.
See [permission failures](#permission-failures-and-storage) below.

## Choose one measurement

Select the feature variant and catalog installation explicitly. For a FlameGraph:

```sh
profile-server-cpu --mode record --features default \
  --sql-fun-home /absolute/path/to/sql-fun-home
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

Private tool preparation and a small mode-specific capture check run before the
expensive build. The command
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

After success, open the latest successful result at its fixed path:

- `target/cpu-reports/record/flamegraph.svg`: latest successful FlameGraph.
- `target/cpu-reports/record/run.json`: metadata for that recording.
- `target/cpu-reports/stat/stat.txt`: latest successful native statistics.
- `target/cpu-reports/stat/run.json`: metadata for those statistics.

With `--output`, these paths are beneath the selected report parent instead.
Each mode publishes independently: running `stat` preserves the published
`record` result, and vice versa. Failed or incomplete runs do not replace the
last successful result. Before a mode's first success its published result is
unavailable. Each mode's report and metadata follow its `latest` directory link;
if reading both while another run finishes, resolve that link once and read
both files from that run directory.

Historical `run-*` directories remain available, including failed runs.
Inspect the printed run directory for the complete evidence:

- `run.json`: overall result, selected mode/features, revision and dirty state,
  kernel, tool/build settings, binary/catalog hashes, server identity, workload,
  capture and report outcomes, timings and report storage.
- `provision.log`, `packages.json`: private tool acquisition and package provenance.
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

For download or tool preparation failures, inspect `provision.log`. Successful
tool preparation does not establish permission to profile. The smoke check
exercises the selected mode and, for `record`, stack extraction and symbolization.
For capture failures inspect `smoke/smoke.log`, the smoke stat reports and
`run.json`. Check the host's `kernel.perf_event_paranoid`, permissions such as
`CAP_PERFMON`, and container seccomp restrictions on `perf_event_open`. A host
administrator must choose any necessary permission changes. A changed host
sysctl alone does not establish that capture works inside a container.

The repository's `ci/github-actions/check` launcher requests `PERFMON` for its
interactive builder, enabling `perf_event_open` under Docker's default seccomp
profile. After updating the launcher, exit the old builder and start a new
session from the host; an already-running container does not acquire the new
setting. The non-root user still needs permission under the host perf policy,
and the command's smoke check remains the test of actual capture capability.

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
