# Workspace test cost policy

The workspace-related nextest routes of `.local/bin/check-test-flow` enforce a
one-second limit per test case. This includes affected-package tests, ordinary
workspace tests, single-archive execution, and the paired CI archive groups.
They explicitly select `ci/nextest/config.toml` through the shared
`run_workspace_nextest` helper. The default, `ci`, `ci-a`, and `ci-b` profiles
use the same policy; CI profiles retain their separate JUnit reports.

`slow-timeout` has a one-second period, `terminate-after = 1`, and a zero-second
grace period. An overlong case is terminated and fails verification rather than
only producing a `SLOW` notice. Do not add longer per-case overrides, retries, or
broad exclusions to accommodate expensive tests. The limit applies to each
case's execution, not compilation or total suite duration. It does not apply to
the separate `proc-macro-tests` step, doctests, or separately invoked CLI/SQL
regression commands.

Review test value as well as execution time. Prefer small existing fixtures and
straightforward reductions in repeated setup when they preserve a worthwhile
assertion. If a test's verification value does not justify its execution and
maintenance cost and its runtime cannot easily be reduced, delete and discard
it, including unused test-only helpers and fixtures. Do not move that cost to
another CI step, keep an ignored/manual copy, or undertake elaborate refactoring
solely to retain the assertion. Record removed coverage and the cost/value
rationale in the pull request. Leave enough runtime margin for the maintained CI
environment; a case barely under one second locally can still fail there.

For issue #828, no additional automated tests, assertions, permanent fixtures,
or CI checks are authorized: CI is billed and no budget is allocated for their
recurring cost. Verify timeout rejection with temporary local inputs using the
maintained invocation and profiles, including archive execution. Record the
results in the pull request and remove those inputs before submission. Use the
existing standard verification flow for the retained tests.

Nextest documents [timeout and termination semantics](https://nexte.st/docs/features/slow-tests/)
and [profile inheritance](https://nexte.st/docs/configuration/).
