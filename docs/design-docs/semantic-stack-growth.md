# Recursive analyzer stack growth

Expression and query semantics keep their recursive structure. A private
`sem::stack::with_stack` policy uses `stacker` checkpoints before entering
recursive conversion and resolution bodies. Wrappers stay thin and bodies use
`#[inline(never)]`, so large branch-local frames are reserved after the check.

## Checkpoint coverage

| Stage | Checkpoint | Recursive paths |
| --- | --- | --- |
| PostgreSQL conversion | `Expr`, `Query`, `FromItem` conversion entry points | Expression operands, query bodies and CTEs, JOIN children, derived queries |
| SQLite conversion | `Expr`, `Query`, `FromItem` conversion entry points | Expression operands, subqueries and CTEs, parenthesized FROM groups |
| Shared resolution | `Expr::resolve_in`, `Query::resolve_in`, `FromItem::resolve` | Expression operands and scalar subqueries, CTEs and set operations, JOIN children and grouped/derived sources |

SQLite's flat JOIN and compound-query lists are built iteratively by existing
conversion code; their resulting recursive trees cross the shared resolution
checkpoints. Direct expression and FROM analysis adapters use the same boundaries.
The outermost schema parse alone is not sufficient: every recursive cycle in
this component must continue to cross a checkpoint as new syntax is supported.

The policy reserves 256 KiB of headroom and allocates 2 MiB segments on demand.
These are internal tuning values, not input-depth or public stack requirements.
Debug-build inspection on Linux x86_64 found approximately 10 KiB frames in
PostgreSQL FROM conversion and shared FROM resolution, approximately 5 KiB in
expression conversion/resolution, and smaller query-entry frames. Headroom also
covers intermediate payload conversions and semantic factories. Continue reducing
unnecessary frame size locally; stack growth does not make finite-stack costs
irrelevant. Recheck headroom when changing large locals or helper call chains.

## Compatibility and limits

The locked dependency is `stacker 0.1.23` (previously transitive through `chumsky`)
with `psm 0.1.30`. `stacker` declares Rust 1.63; the complete selected dependency
tree is checked using this workspace's Rust 1.93.1 minimum. Upstream is maintained
under `rust-lang/stacker`, with backend/CI maintenance visible in its
[August 2026 update](https://github.com/rust-lang/stacker/commit/d3ac2df79a409c681329006e9b31c96ab9c5fcae).

| Target | Stack backend | Verification in this change |
| --- | --- | --- |
| `x86_64-unknown-linux-gnu` | `psm` stack switching | Local small-stack conversion/resolution and forced-growth unwind tests; repository CI target |
| Other native targets | Platform-dependent `psm`, or Windows Fibers | Upstream support only; not validated by this repository change |
| Unsupported backends | No effective stack growth | No protection claim |

See the [upstream support guidance](https://github.com/rust-lang/stacker#platform-support).
Growth keeps execution on the same thread, including thread-local state, and
returns through the existing result/diagnostic paths. Allocation failure, parser
limits and total memory exhaustion remain possible. Recursive cloning, dropping,
serialization and other AST consumers are not comprehensively instrumented.

## Regression strategy

Abort-prone tests run their own exact test case in a subprocess and analyze on
explicit worker stacks. This prevents `RUST_MIN_STACK` from masking failures and
keeps an overflow from aborting the parent suite. The unchanged AdventureWorks
schema includes PostgreSQL 17 builtin and tablefunc bootstrap and asserts all
employee-view output columns and the employee-id type. Separate cases cover both
JOIN directions, nested expressions, scalar/derived subqueries, CTE bodies, set operations
and a normal unresolved-column diagnostic for both frontends. Parsing/conversion
uses a separate 2 MiB worker; resolution runs with both 512 KiB and 2 MiB.
Disabling the policy causes the SQLite left/right-deep JOIN cases to overflow on
512 KiB; the corresponding 28 cases pass with growth. Additional scalar-subquery cases
cover the expression-to-query recursion edge. This isolates the improvement from parser
stack requirements and verifies conversion independently of Cargo's stack default. Forced growth checks
same-thread execution, normal return, panic propagation and destructor execution.

The baseline after the shared unresolved-AST refactor already analyzes
AdventureWorks on 2 MiB. This change preserves that behavior and adds stack-growth
protection rather than claiming the original historical backtrace still occurs.
