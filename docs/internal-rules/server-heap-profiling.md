# Local server heap profiling

Use this optional development command to find allocation sites in
`sql-fun-server` across the complete CI SQL regression workload. Profiling runs
locally and is not part of CI or normal verification. It preserves the existing
manifest, expectations, two-worker scheduling, isolation revisits and negative
controls. SQL is analyzed against the selected real builtin catalogs; it is not
executed against a live database.

## Run a measurement

Use the contributor environment described in [CONTRIBUTING.md](../../CONTRIBUTING.md).
The initial supported platform is Ubuntu 24.04 amd64 with its existing Cargo, uv,
Bash, jq, C compiler with static libc support, gzip, APT, dpkg and Ubuntu archive
signing key. Network access to the Ubuntu archives is required. Build, report
and scratch space usage depends on the workload; filesystem errors are reported
when an operation fails.

You do **not** need to install heaptrack manually. Every invocation downloads
and extracts heaptrack, heaptrack_print, binutils (for readable Rust symbols)
and their runtime dependency packages
into its own temporary directory. It verifies authenticated Ubuntu repository
metadata and package hashes, runs a small symbolization/capture check, and
records the selected package versions. It does not install host packages, run
package installation scripts, require sudo, or change the CI image.

Explicitly select the feature variant, builtin catalog installation, persistent
report parent and temporary scratch parent:

```sh
.local/bin/profile-server-heap --features default \
  --sql-fun-home "$GIT_ROOT/sql-fun-home" \
  --output "$GIT_ROOT/target/heap-reports" \
  --raw-dir "$GIT_ROOT/target/heap-scratch"
```

Use absolute paths without quotes, backslashes, percent signs or newlines in
their names (heaptrack interprets percent placeholders in output paths).
The command uses the current checkout's committed fixture manifest and priority
file. It validates PostgreSQL 17/18 catalog metadata in the installation you
selected. It never attaches to a developer's existing server.

Run again with `--features all` to measure the all-features CI variant. Each
invocation creates fresh `run-*` reports and `heap-profile-*` scratch directories.
The command builds the selected CLI and server with locked release dependencies,
release optimization, full debug information and no stripping. Cargo's target
directory setting is respected. Build settings apply only to that invocation.

The profiler captures server startup, the full workload, and natural shutdown.
After the workload finishes, allow approximately three minutes for the server's
ordinary client-free idle shutdown, followed by profile finalization and
extraction. The finalization wait is bounded at four minutes. Forced termination
is recorded as an incomplete capture, retains available raw data and returns
failure. A failed suite still attempts extraction but returns failure even when
its profile is readable. Instrumentation changes runtime and memory use; these
measurements are not normal server latency or RSS benchmarks.

## Read the results

The command prints the report directory. Open `report.txt` in a text editor;
long optimized Rust frames are easier to inspect without line wrapping. Inspect:

- `report.txt`: native heaptrack text, with allocation counts, temporary
  allocations, peak consumers, unfreed allocations and top call stacks (20 per
  category), passed through the standard `c++filt --format=rust` filter so current
  Rust symbol names are readable. Backtrace merging is disabled because heaptrack
  warns that merged
  peak figures are incorrect. Suppressions are disabled.
- `run.json`: revision and dirty-tree status, build/features, binary and catalog
  metadata hashes, suite outcome, capture/extraction state, durations and sizes.
- `packages.json`, `tools.json`: provisioning provenance and selected versions.
- `suite/`: original worker/JUnit logs, aggregate results, claims, isolation
  revisits and negative-control evidence. Expected negative-control failures do
  not mean the regression suite failed.

Heaptrack's native “leaks” label means allocations unfreed **at capture end**.
It is not proof of an application leak. Peak heap bytes are not cumulative
allocated bytes. The Ubuntu 1.5 printer does not provide cumulative allocated-byte
statistics or byte-ranked allocation sites; the report marks these unavailable.
Peak RSS includes profiler overhead. Reports retain stacks and source locations
when available, but optimized/inlined Rust frames may be combined or absent.

Reports are published only after successful extraction and checks for supported
sections. A successful normal capture/extraction deletes that run's bulky raw
profile, private tools and package cache. Existing reports are never overwritten.

## Inspect raw data or recover a failed extraction

Add `--keep-raw` when invoking the command to retain raw data and private tools
for investigation. On extraction failure they are retained automatically.
`recovery.txt` in the report directory contains an exact native printer command
with the retained library paths, and a command to remove only that run's scratch
directory after inspection. No persistent heaptrack installation is needed.

A failure before extraction also prints the retained scratch path. Inspect
`run.json`, `provision.log`, `smoke.log`, `build.log` or `server.log` as appropriate;
an incomplete capture is not a successful measurement. Do not delete another
run's scratch data. Package/provisioning logs and compact run metadata remain
available even when no valid report was produced.

## Measurement evidence

For an initial measurement, retain the report and summarize provisioning/smoke,
build, workload/startup, shutdown/finalization and extraction durations separately.
`run.json` records the compressed raw size before deletion (the capture file grows
through the run), temporary storage including provisioned packages, and report
storage. Compare those costs with the usefulness of the symbolized allocation
stacks before proposing any recurring execution. Adding profiling to CI or
normal local checks requires a separate maintainer decision.

The coordinator lives under `.local/sbin/heap-profile/`. It reuses the existing
workload functions without changing CI's server lifecycle or execution paths.
Its short sequence of full groups, isolation revisits and controls must be
reviewed against `tests/sqlast-regressions/parallel_runner.py` when that workload
coordination changes. It does not regenerate committed priorities.
