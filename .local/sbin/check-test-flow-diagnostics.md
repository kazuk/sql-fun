# Check-flow diagnostic extraction

`check-test-flow-diagnostics.py` is an internal helper invoked by agent
presentation after a step finishes, with the repository Rust matcher JSON and
the raw step log as its two arguments. It uses only the Python 3 standard library.

Rust diagnostic headers come from each matcher's first pattern in
`.github/problem-matchers/rust.json`. Subsequent patterns and Rust source/help
lines provide location and context. Incomplete headers remain visible. Matching
ignores ANSI escapes; emitted diagnostics retain their original text. A missing
final newline is added to the emitted diagnostic, leaving the log unchanged.

Additional patterns recognize current Markdown lint, typos, cargo-deny, taplo,
GitHub annotation commands, Rust panic headers, and missing-tool messages.
Unrecognized formats remain in the raw log. Extend the patterns and fixtures
when a new tool has actionable diagnostics that should appear on agent stderr.

Maintainers can run the internal regression command from the repository root:

```sh
.local/bin/test-check-test-flow
```

The fixtures exercise the command in temporary repositories with fake tools;
they also verify Python compatibility with the Rust matcher regular expressions.
For a local comparison with an explicitly supplied pre-change script, set
`FLOW_TEST_BASELINE` to that script's absolute path. The optional comparison
checks commands, ordering, streams, and exit codes under `CI=true`; standard
runs skip this comparison and assert the enduring execution contract directly.
