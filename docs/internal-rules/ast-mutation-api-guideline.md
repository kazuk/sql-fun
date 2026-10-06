# AST mutation API guidelines

Read this document before planning, implementing, or reviewing AST mutation
APIs, including setters, collection operations, and editors that borrow an AST.
It is for contributors deciding how an edit becomes part of the owning AST and
how callers observe success, failure, and abandonment. It supplements the
[Rust coding guideline](rust-coding-guideline.md) and the
[Explicit-choice Philosophy](../../book/design-philosophies.md#explicit-choice-philosophy).

## Define the state transitions before implementing the edit

Describe the initial state, the state visible during editing, and the state
after each exit path. Include both absent and existing optional nodes. An editor
being created, a mutation succeeding, and an editor being dropped are distinct
events; decide which event commits each change to the owner.

For every operation, specify:

- The representation, dialect, and placement invariants it preserves. State
  separately which checks, such as name resolution, require semantic analysis.
- Whether changes apply immediately or remain pending until a documented commit
  point, such as the first successful insertion or an explicit commit operation.
- What immutable accessors report before and after the operation, and whether an
  editor's view includes pending state.
- What remains after an error, early return, or abandonment of the editor.
- Whether the caller can retry after failure and reopen the editor afterwards.

Do not assume that a method named `ensure_*` is harmless merely because it
returns an editor. Creating an optional child, changing a flag, or initializing
metadata can change the owner before the caller completes the intended edit.

## Keep committed state consistent with its public meaning

The owning AST must retain the invariants promised by its mutation API whenever
control returns to the caller. A pending draft may be incomplete, but it must be
kept separate from committed state and identified as pending in the API contract.

If an immutable accessor reports that a clause is absent, there must not be
hidden clause settings that affect a later edit or another consumer of the AST.
An internal empty value can represent absence, provided its metadata is also in
the canonical absent state. Hiding a node in an accessor does not repair an
invalid or inconsistent owner.

When an operation removes the last child, define the resulting state as well.
If the contract makes the optional clause absent, remove or reset metadata that
belongs only to that clause in the same operation. If the requested result
cannot satisfy the operation's invariants, reject the edit without changing the
owner.

Preserve unaffected order, attributes, and dialect-specific constructs. Require
explicit choices for changes such as enabling recursion or selecting insertion
order; do not silently repair an invalid request by choosing for the caller.

## Choose a commit boundary that survives abandonment

Use immediate mutation when every successful operation leaves the owner in a
valid state according to the API's contract. When a new optional node needs
several steps before it can be attached, keep the incomplete node in the editor
and attach it only after validation succeeds at the documented commit point.
Prefer staging the smallest affected subtree over copying the entire AST.

Validate the proposed edit in its destination context before changing committed
state. Validation must cover all relevant children, including subtrees copied
from an existing AST. Encapsulate the final state change so there is no fallible
validation step after part of the edit has already been committed.

Do not rely on `Drop` as the only mechanism that restores AST invariants. A
caller can forget an editor without running its destructor. Early returns and,
where applicable, cancellation or unwinding also need a deliberate exit path.
Keeping incomplete state in the editor lets abandonment leave the owner intact
without a rollback destructor. State and test any stronger panic or cancellation
guarantees separately from ordinary `Result` failure guarantees.

Failure atomicity is normally per operation: returning `Err` leaves the owner
as it was immediately before that call. Earlier successful edits remain applied
for an editor with immediate mutation. A transaction that rolls back an entire
editing session is a separate contract and should be introduced when the use
case requires it. For staged operations, also specify whether failure preserves
the draft so that the caller can retry.

## Example: creating an optional WITH clause

The `ensure_with_clause()` review in
[PR #370](https://github.com/kazuk/sql-fun/pull/370#discussion_r4190038222)
found that setting `recursive = true` through an empty editor changed the owner.
After the editor was dropped, `with_clause()` reported `None`, but opening a new
editor exposed the retained flag. No CTE had ever been inserted.

The corrected contract illustrates separate pending and committed state:

| Initial owner | Action | Result |
| --- | --- | --- |
| WITH absent | Open editor and enable recursion | Owner stays absent; editor view shows the pending choice. |
| WITH absent | First insertion fails | Owner stays absent; pending choice remains available for retry. |
| WITH absent | Abandon or forget the empty editor | Owner stays absent; reopening starts without the abandoned choice. |
| WITH absent | First insertion succeeds | CTE and recursive choice are committed together. |
| WITH present | Change a flag or insert successfully | Change applies immediately and remains after the editor ends. |
| WITH present | A later operation fails | State from earlier successful operations is preserved. |

This is one commit policy, not a requirement that every editor use first
insertion as its commit point. Document the appropriate policy for each API.
In particular, a future removal operation must define what happens when a
populated clause becomes empty again.

## Verify complete edit lifecycles through the public API

Tests should observe the owner after the editor's borrow ends, then reopen it or
perform another supported operation. Checking only the editor's view can miss
hidden state; checking only `is_err()` can miss a partial mutation.

Cover the relevant transitions for the changed API:

- Open and abandon an editor without edits.
- Change a setting before creating the first child, then abandon the editor.
- Attempt an invalid edit, inspect the failure reason, and confirm that the
  owner and any retained draft match the documented failure contract.
- Retry after failure and confirm the successful result becomes observable.
- Commit one edit, fail a later edit, and confirm the earlier result survives
  when the API uses immediate mutation.
- Reopen the editor after abandonment or success and check for stale state.
- For an editor whose abandonment guarantee depends on ownership, forget it
  and verify that the owner still satisfies its invariants.
- For removal or replacement APIs, include the last-child case and changes to
  metadata associated with that child or clause.

Exercise absent and existing nodes and the applicable dialects. Use focused
cases rather than expanding a matrix of unrelated syntax. Prefer public
inspection and subsequent supported operations to private-field assertions or
`Debug` snapshots. Follow the Rust guideline for test placement, helpers, and
parameterization. Syntax corpus tests complement these lifecycle tests; they do
not replace them.

## Review and documentation checklist

- Is the commit point explicit, including the difference between an absent node
  being prepared and an existing node being edited?
- Do normal completion, errors, and abandonment preserve the owner's invariants
  without requiring destructor cleanup?
- Do immutable views, editor views, and subsequent operations agree about what
  is committed and what remains pending?
- Does every rejected operation preserve the promised state, including earlier
  successful edits and the draft state needed for retry?
- Can callers identify the failure reason through the supported error API?
- Do insertion, replacement, and removal preserve relevant attributes and
  validate the destination context without making omitted choices?
- Do tests inspect the owner after editing ends and cover abandonment as well
  as success and failure?
- Does public rustdoc explain observable commit, error, and abandonment behavior?
  Keep storage choices and implementation rationale in internal documentation.

A passing build and syntax corpus do not establish these state guarantees.
Review the ownership and commit paths alongside the lifecycle tests.
