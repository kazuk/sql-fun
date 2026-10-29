# Shared-server CLI regression experiment findings (#870)

## Evidence and reproduction

These are local feasibility observations for maintainers deciding whether to
adopt parallel regression execution. CI recommendations and final-head run links
are recorded in the implementation PR linked from
[issue #870](https://github.com/kazuk/sql-fun/issues/870). The PR's CI benchmark
artifact retains full worker reports, queue claims, server logs, resource samples,
and comparison summaries for 30 days. The PR records the measured CI summary and
recommendation permanently; local observations do not select the CI default.

The [local measurements](evidence/parallel-local-results.json) retain every
comparison sample, worker durations and exits, outcomes, resource observations,
and all calibration estimates. The measured source revision was
`dee87637575fd7681ec79c96322ed2b5a3b613c4`. See the
[suite README](README.md#opt-in-shared-server-parallel-experiment-870)
for the explicit binary/catalog selections and reproduction command.

The implementation subsequently incorporated #869's CI job separation. That
integration did not change the measured harness, production binaries, catalogs,
or regression fixtures. CI measurements use the final PR head.

Local configuration:

- CPU: AMD Ryzen 7 5800X 8-Core Processor; 16 logical CPUs visible.
- Toolchain: rustc 1.99.1 (58cf8f9c3 2026-10-13).
- Interpreter: Python 3.12.3.
- One default-feature CLI/server pair, built once and reused across trials.
- Same 140 fixture/version groups and 2,212 named cases in each positive suite.
- No concurrent builds or coverage jobs. Linux page caches were not cleared.
- Fresh owned server for every trial; canonical fixture paths stayed fixed.
- One separate serial calibration, then three trials of each worker count.
  Configuration order rotated; every comparison used the same frozen estimates.

## Local measurements

Times are seconds. Full wall time includes server startup, worker processing,
aggregation, 34 schema/role revisit cases, controls, and cleanup. RSS is the
maximum observed server peak RSS across the three trials, in MiB.

| Workers | Median wall | Wall range | Speedup | Peak RSS |
| --- | --- | --- | --- | --- |
| 1 | 602.577 | 573.256–610.724 | 1.00× | 340.5 |
| 2 | 308.356 | 294.089–309.680 | 1.95× | 369.0 |
| 4 | 161.315 | 161.084–163.282 | 3.74× | 424.7 |

The separate calibration is excluded from these comparisons. The JSON evidence
also records positive-worker-phase times, CLI command-time sums and throughput.
Summed command time is not parallel wall time.

Median sampled server CPU seconds divided by full wall seconds:

- 1 worker: 0.99
- 2 workers: 1.98
- 4 workers: 3.91

These process-level CPU observations support concurrent server execution. They
do not identify a particular lock or schema-load bottleneck. RSS samples describe
this workload and do not establish a general memory bound.

## Correctness and limitations

All nine comparison trials and the separate calibration passed exact group/case
identity reconciliation, declared-counter validation, worker exits, schema/role
revisits, and negative controls. Every positive suite contained 2,212 cases with
zero failures, errors, or skips; each same-server revisit contained 34 passing
cases. No server registration replacement, crash, or schema-isolation failure was
observed. This evidence does not prove absence of concurrency bugs. Any confirmed
server concurrency defect is to receive a separate issue linked to #870.

The shared queue dispatches longer estimated groups first. Workers claim tasks
atomically and drain the remaining queue; they do not have static partitions.
Completed claims cover every expected group once. Independent fast contracts
verify missing/duplicate reports and identities, worker death, malformed XML,
counter mismatches, failure/error details, and skipped-element preservation.
The current product does not emit skipped cases.

The cache retains contexts for the server lifetime. Revisits verify observable
results after other selections; cache hit/miss counts and lock wait times are not
instrumented. Existing expectations remain unchanged, and no SQL cases are
omitted to obtain speedups. Three samples quantify observed variation rather
than guaranteeing a result on different hardware or future manifests.

## CI decision boundary

The opt-in CI job measures a separate serial calibration and three trials each
of 1/2/4/8 workers on the normal workspace runner/container, using CLI/server
artifacts from that same run. Evaluate end-to-end time, variation, correctness,
resource use, and scheduling/reporting overhead, including CPU oversubscription.
Use actual CI evidence in the PR to recommend a stable worker count.

Ordinary regression execution remains serial. Review the recommendation and
additional CI/runtime cost explicitly before adopting a default, coordinated
with #869's job separation. Discovering a server defect authorizes reporting it;
repairs and unapproved workarounds are outside this experiment's scope.
