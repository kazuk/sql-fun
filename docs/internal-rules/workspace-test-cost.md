# Workspace test cost policy

The workspace-related nextest routes of `.local/bin/check-test-flow` include
affected-package tests, ordinary workspace tests, single-archive execution, and
the paired CI archive groups. Local runs terminate a case after one second.
They explicitly select `ci/nextest/config.toml` through the shared
`run_workspace_nextest` helper. The `ci`, `ci-a`, and `ci-b` profiles report
cases running for at least one second as `SLOW` and terminate/fail cases at two
seconds. A `SLOW` notice alone does not fail CI. CI profiles retain their
separate JUnit reports and nextest's default parallel execution. Their final
status lists slow case names even when those cases pass.

The local default profile uses a one-second `slow-timeout` period with
`terminate-after = 1` and zero grace. CI uses the same one-second period with
`terminate-after = 2` and zero grace. A test reaching two seconds is terminated
by nextest and fails the case and CI job. Do not add retries or broad exclusions
to accommodate expensive tests. These limits apply to each case's execution,
not compilation or total suite duration.
They do not apply to the separate `proc-macro-tests` step, doctests, or
separately invoked CLI/SQL regression commands.

Review test value as well as execution time. Prefer small existing fixtures and
straightforward reductions in repeated setup when they preserve a worthwhile
assertion. If a test's verification value does not justify its execution and
maintenance cost and its runtime cannot easily be reduced, delete and discard
it, including unused test-only helpers and fixtures. Do not move that cost to
another CI step, keep an ignored/manual copy, or undertake elaborate refactoring
solely to retain the assertion. Record removed coverage and the cost/value
rationale in the pull request. Follow the test provenance and review-versus-merge
rules in `AGENTS.md` when a CI warning or timeout appears. A new test's `SLOW`
warning must be fixed before merge, even when CI passes.

For issue #828, no additional automated tests, assertions, permanent fixtures,
or CI checks are authorized: CI is billed and no budget is allocated for their
recurring cost. Verify timeout rejection with temporary local inputs using the
maintained invocation and profiles, including archive execution. Record the
results in the pull request and remove those inputs before submission. Use the
existing standard verification flow for the retained tests.

Nextest documents [timeout and termination semantics](https://nexte.st/docs/features/slow-tests/)
and [profile inheritance](https://nexte.st/docs/configuration/).
