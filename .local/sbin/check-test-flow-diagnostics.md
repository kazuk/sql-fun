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

The helper is required only for agent-mode diagnostic presentation. It must
remain compatible with the repository's Rust matcher regular expressions and
must leave the raw step log unchanged.
