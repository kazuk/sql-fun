# Issue and pull request label rules

These rules define the workflow meaning of labels used on issues and pull
requests. Keep label meaning stable so repository state can be understood and
queried without reconstructing intent from individual conversations.

Labels in this document describe workflow state or dependency relationships.
They are independent from priority unless explicitly stated otherwise.

## `bug`

Use `bug` for an issue that describes behavior which is incorrect relative to
the intended or documented behavior of the repository.

Typical cases include:

- a valid input is rejected or handled incorrectly;
- generated code is invalid or behaves incorrectly;
- behavior differs from the documented contract;
- a regression breaks behavior that previously worked;
- implementation details cause observable failures such as identifier
  collisions, incorrect type constraints, or invalid semantic analysis.

Do not use `bug` merely because functionality is not implemented yet. Missing
or planned functionality should be tracked as feature or implementation work
unless existing behavior claims that functionality should already work.

`bug` is orthogonal to workflow-state labels such as `not-ready`,
`blocked`, and `blocker`, and is also independent from priority.

## `not-ready`

Use `not-ready` when work must not start yet because the task is still being
prepared.

Typical reasons include:

- requirements are still being collected;
- acceptance criteria are incomplete;
- scope or implementation boundaries are not decided;
- investigation is still required before the task can be implemented safely.

The issue body must state why the task is not ready and what remains to make it
ready. Prefer a dedicated `Pending` section when the reason is not otherwise
obvious.

Remove `not-ready` only when implementation can begin without first making
additional product or design decisions.

Do not use `blocked` merely because a task is still being designed.
`not-ready` means the task itself is not prepared for implementation.

## `require-human-work`

Use `require-human-work` when progress is currently stopped because the
remaining operation cannot be completed by the repository's available agents
or automation.

Typical reasons include:

- a required API or connector operation is unavailable;
- repository or service permissions prevent an agent from performing the
  operation;
- a required action is available only through a human-operated UI;
- authentication, approval, billing, or account-level settings require a human
  owner;
- another platform limitation prevents the mechanical step from being
  completed automatically.

Record the specific human action required and why automation cannot currently
perform it in a comment. Prefer a new comment when the required action changes,
so the history of automation boundaries remains visible without repeatedly
editing the issue or pull request body.

Do not use `require-human-work` merely because human review is desirable or
because a task is difficult. It marks a concrete automation boundary.

`require-human-work` may be combined with `blocked` when the missing human
action is the blocker preventing an otherwise ready task from proceeding.

Remove `require-human-work` as soon as the human-only action has been
completed or the automation limitation no longer applies. Removal does not need
to be performed by the human who completed the action: an agent or automation
should remove the label when it can verify that work can proceed again. Prefer
clearing stale labels opportunistically during continued work rather than
waiting for a human to remember to remove them.

## `blocked`

Use `blocked` when the task is otherwise ready to proceed but a concrete
dependency or obstacle prevents progress.

The issue or pull request body must identify the blocker when known, for
example:

```markdown
Blocked by #123.
```

Examples include:

- another repository issue must be completed first;
- an upstream release or specification is required;
- a required permission or infrastructure change is pending;
- an external dependency prevents implementation or merge.

Do not use `blocked` for unspecified uncertainty. If the implementation
requirements are not yet ready, use `not-ready` instead.

## `blocker`

When a repository-internal issue is the reason another issue or pull request is
`blocked`, label the dependency issue `blocker`.

Record the relationship in both directions when practical:

```text
#123  blocker   -- Blocks #456.
#456  blocked   -- Blocked by #123.
```

`blocker` describes dependency-graph impact, not intrinsic priority. A
low-priority issue can still be a blocker and may need to be resolved early
because leaving it open reduces repository-wide development throughput.

Do not use `blocker` merely as a synonym for high priority or severity.

## Priority is independent

Priority and dependency state are separate axes.

For example, all of the following combinations are valid:

```text
priority: low  + blocker
priority: high + blocked
priority: low  + not-ready
```

Choose priority according to the task's own value or urgency. Use
`not-ready`, `blocked`, and `blocker` to describe whether work can proceed
and how it affects other work.

## State transitions

A common lifecycle is:

```text
not-ready
  -> ready
  -> blocked
  -> ready / in-progress
  -> completed
```

A task does not need to pass through every state. In particular, use
`blocked` only after a concrete blocker is known.

When a blocker is resolved:

- remove `blocked` from tasks that can proceed;
- remove `blocker` from the dependency issue when it no longer blocks any
  repository work;
- update stale `Blocked by` / `Blocks` references when necessary.
