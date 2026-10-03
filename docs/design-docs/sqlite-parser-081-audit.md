# SQLite syntax adapter audit for turso_parser 0.8.1

This inventory was prepared against the published 0.7.2 `src/ast.rs` before
changing the dependency. It records the complete AST type inventory, including
planner-only variants, so compiler errors are not the sole compatibility check.
Published crate sources, rather than upstream main, define this boundary.

## Baseline findings

- `Update` dropped `from`, `where_clause`, `returning`, `order_by`, and `limit`.
  Restore all five before upgrading; `from` is named `from_clause` in the owned
  IR to avoid a collision with the `From` conversion.
- `Type` dropped `array_dimensions`. Restore it for column, CAST, and custom
  type fields; semantic support for array types remains outside this change.
- The boundary retained upstream `Cmd` until semantic dispatch, which erased
  command kind. Add an owned command adapter during the upgrade.
- All other command-reachable fields and enum payloads have a corresponding
  stored representation. Collections retain order, optional fields retain
  absence, and owned strings retain their parser-provided contents.
- `Name` stores value and quote metadata. The 0.7.2 parser itself discards
  bracket quoting and BLOB prefix spelling before creating the AST; that is
  different from sql-fun losing parser-provided information.
- `JoinType` and `TableInternalId` wrap the upstream value without discarding
  bits or numeric identity. `SubqueryType::In` changes `Arc<String>` ownership
  to `String`, retaining its contents.
- `ParameterInfo`, `PragmaName`, and `TableOptionsFlag` are parser metadata or
  helpers, not fields reachable from `Cmd`. Generic pragma names/bodies and
  `TableOptions` spelling preserve the corresponding syntax.

## Published 0.7.2 inventory

Each row lists every stored field or variant. Enum payloads are converted
one-to-one, including all nested fields enumerated in the upstream definition.
For `Stmt` and `Expr`, variant-specific payload conversion resides in the
listed module and its dedicated payload modules. The repairs above are the
exceptions found in the pre-upgrade representation.

| Upstream type | Fields or variants | Adapter location |
| --- | --- | --- |
| `ParameterInfo` | count, names | `parser metadata (not reachable from Cmd)` |
| `Cmd` | Explain, ExplainQueryPlan, Stmt | `command.rs (added during upgrade)` |
| `CreateVirtualTable` | if_not_exists, tbl_name, module_name, args | `table_management_statement.rs` |
| `Update` | with, or_conflict, tbl_name, indexed, sets, from, where_clause, returning, order_by, limit | `update_statement.rs` |
| `AlterTable` | name, body | `table_management_statement.rs` |
| `Stmt` | AlterTable, Analyze, Attach, Begin, Commit, CreateIndex, CreateTable, CreateTrigger, CreateView, CreateMaterializedView, CreateVirtualTable, CreateType, CreateDomain, Delete, Detach, DropIndex, DropTable, DropTrigger, DropView, DropType, DropDomain, Insert, Pragma, Reindex, Release, Rollback, Savepoint, Select, Update, Vacuum, Optimize, CreateSequence, DropSequence | `../sqlite.rs` |
| `TableInternalId` | numeric identity | `table_internal_id.rs` |
| `FieldAccessResolution` | StructField, UnionVariant | `expr/field_access.rs` |
| `Expr` | Between, Binary, Register, Case, Cast, Collate, DoublyQualified, Exists, FieldAccess, FunctionCall, FunctionCallStar, Id, Column, RowId, InList, InSelect, InTable, IsNull, Like, Literal, Name, NotNull, Parenthesized, Qualified, Raise, Subquery, Unary, Variable, SubqueryResult, Default, Array, Subscript | `expr.rs` |
| `Variable` | index, name, col_type | `expr/variable.rs` |
| `SubqueryType` | Exists, RowValue, In | `subquery_type.rs` |
| `Literal` | Numeric, String, Blob, Keyword, Null, True, False, CurrentDate, CurrentTime, CurrentTimestamp | `literal.rs` |
| `LikeOperator` | Glob, Like, Match, Regexp | `expr/like.rs` |
| `Operator` | Add, And, ArrowRight, ArrowRightShift, BitwiseAnd, BitwiseOr, BitwiseNot, Concat, Equals, Divide, Greater, GreaterEquals, Is, IsNot, LeftShift, Less, LessEquals, Modulus, Multiply, NotEquals, Or, RightShift, Subtract, ArrayContains, ArrayOverlap | `expr/binary.rs` |
| `UnaryOperator` | BitwiseNot, Negative, Not, Positive | `expr/unary.rs` |
| `Select` | with, body, order_by, limit | `select.rs` |
| `SelectBody` | select, compounds | `select.rs` |
| `CompoundSelect` | operator, select | `select.rs` |
| `CompoundOperator` | Union, UnionAll, Except, Intersect | `select.rs` |
| `OneSelect` | Select, Values | `select.rs` |
| `FromClause` | select, joins | `from_clause.rs` |
| `Distinctness` | Distinct, All | `select.rs` |
| `ResultColumn` | Expr, Star, TableStar | `select.rs` |
| `As` | As, Elided, ImplicitColumnName | `payload module (see parent conversion)` |
| `JoinedSelectTable` | operator, table, constraint | `from_clause.rs` |
| `SelectTable` | Table, TableCall, Select, Sub | `from_clause.rs` |
| `JoinOperator` | Comma, TypedJoin | `from_clause.rs` |
| `JoinType` | bitflags: INNER, CROSS, NATURAL, LEFT, RIGHT, OUTER | `from_clause.rs` |
| `JoinConstraint` | On, Using | `from_clause.rs` |
| `GroupBy` | exprs, having | `group_by.rs` |
| `Name` | quote, value | `names.rs` |
| `QualifiedName` | db_name, name, alias | `names.rs` |
| `AlterTableBody` | RenameTo, AddColumn, AlterColumn, RenameColumn, DropColumn | `table_management_statement.rs` |
| `TypeOperator` | op, func_name | `type_statements.rs` |
| `TypeParam` | name, ty | `type_statements.rs` |
| `DomainConstraint` | name, check | `domain.rs` |
| `CreateTypeBody` | CustomType, Struct, Union | `type_statements.rs` |
| `CreateTableBody` | ColumnsAndConstraints, AsSelect | `table_management_statement.rs` |
| `ColumnDefinition` | col_name, col_type, constraints | `column_definition.rs` |
| `NamedColumnConstraint` | name, constraint | `table_management_statement.rs` |
| `ColumnConstraint` | PrimaryKey, NotNull, Unique, Check, Default, Collate, ForeignKey, Generated | `constraints.rs` |
| `GeneratedColumnType` | Stored, Virtual | `constraints.rs` |
| `NamedTableConstraint` | name, constraint | `table_management_statement.rs` |
| `TableConstraint` | PrimaryKey, Unique, Check, ForeignKey | `constraints.rs` |
| `TableOptions` | without_rowid_text, strict_text | `table_management_statement.rs` |
| `TableOptionsFlag` | WithoutRowid, Strict | `table_management_statement.rs (helper; TableOptions retains spelling)` |
| `SortOrder` | Asc, Desc | `sorted_column.rs` |
| `NullsOrder` | First, Last | `sorted_column.rs` |
| `ForeignKeyClause` | tbl_name, columns, args | `constraints.rs` |
| `RefArg` | OnDelete, OnInsert, OnUpdate, Match | `constraints.rs` |
| `RefAct` | SetNull, SetDefault, Cascade, Restrict, NoAction | `constraints.rs` |
| `DeferSubclause` | deferrable, init_deferred | `constraints.rs` |
| `InitDeferredPred` | InitiallyDeferred, InitiallyImmediate | `constraints.rs` |
| `IndexedColumn` | col_name, collation_name, order | `select.rs` |
| `Indexed` | IndexedBy, NotIndexed | `from_clause.rs` |
| `SortedColumn` | expr, order, nulls | `sorted_column.rs` |
| `Limit` | expr, offset | `select.rs` |
| `InsertBody` | Select, DefaultValues | `insert_statement.rs` |
| `Set` | col_names, expr | `update_statement.rs` |
| `PragmaBody` | Equals, Call | `pragma.rs` |
| `PragmaName` | ApplicationId, AutoVacuum, BusyTimeout, CacheSize, CacheSpill, EncryptionCipher, DataSyncRetry, DatabaseList, Encoding, FreelistCount, ForeignKeys, ForeignKeyList, FullColumnNames, FunctionList, Fullfsync, IgnoreCheckConstraints, IntegrityCheck, JournalMode, LockingMode, QuickCheck, EncryptionKey, LegacyFileFormat, MaxPageCount, ModuleList, PageCount, PageSize, QueryOnly, SchemaVersion, ShortColumnNames, IAmADummy, RequireWhere, Synchronous, TempStore, IndexInfo, IndexXinfo, IndexList, TableList, TableInfo, TableXinfo, CaptureDataChangesConn, UnstableCaptureDataChangesConn, UserVersion, WalCheckpoint, MvccCheckpointThreshold, MvccGcThreshold, ListTypes, EmptyResultCallbacks, VdbeTrace | `pragma.rs (generic name/body)` |
| `TriggerTime` | Before, After, InsteadOf | `trigger_statement.rs` |
| `TriggerEvent` | Delete, Insert, Update, UpdateOf | `trigger_statement.rs` |
| `TriggerCmd` | Update, Insert, Delete, Select | `trigger_statement.rs` |
| `ResolveType` | Rollback, Abort, Fail, Ignore, Replace | `resolve_type.rs` |
| `With` | recursive, ctes | `with.rs` |
| `Materialized` | Any, Yes, No | `with.rs` |
| `CommonTableExpr` | tbl_name, columns, materialized, select | `with.rs` |
| `TypeField` | name, field_type | `type_statements.rs` |
| `Type` | name, size, array_dimensions | `type_.rs` |
| `TypeSize` | MaxSize, TypeSize | `type_.rs` |
| `TransactionType` | Deferred, Immediate, Exclusive, Concurrent | `transaction_statement.rs` |
| `Upsert` | index, do_clause, next | `upsert.rs` |
| `UpsertIndex` | targets, where_clause | `upsert.rs` |
| `UpsertDo` | Set, Nothing | `upsert.rs` |
| `FunctionTail` | filter_clause, over_clause | `function_tail.rs` |
| `Over` | Window, Name | `over.rs` |
| `WindowDef` | name, window | `window.rs` |
| `Window` | base, partition_by, order_by, frame_clause | `window.rs` |
| `FrameClause` | mode, start, end, exclude | `frame_clause.rs` |
| `FrameMode` | Groups, Range, Rows | `frame_clause.rs` |
| `FrameBound` | CurrentRow, Following, Preceding, UnboundedFollowing, UnboundedPreceding | `frame_clause.rs` |
| `FrameExclude` | NoOthers, CurrentRow, Group, Ties | `frame_clause.rs` |

## Reconciliation with published 0.8.1

| Change | Disposition |
| --- | --- |
| EQP structured payload and EqpFormat | Owned command kind, statement, and Text/Json format |
| UPDATE/DELETE order_by and limit removed | Remove those fields after baseline regression validation; document inherited rejection |
| Variable.numbered | Preserve alongside index/name/col_type; distinguish bare and numbered markers |
| Column/TableConstraint CHECK source | Preserve optional source verbatim in both payloads |
| Generated.generated_always | Preserve independently of expression and optional storage type |
| Name bracket metadata and rendering | Read quote predicates directly; update bracket expectations |
| BLOB lexer supplies complete spelling | Retain spelling; derive hex payload for existing semantic consumers |
| New pragma names | Retain qualified generic name/body; classify Turso extensions through hooks |
| Parenthesized IN subqueries | Assert owned InSelect and unresolved membership/negation |
| Named parameter lexer grammar | Retain complete prefix, namespace, and parenthesized suffix |
| Generated/compound SELECT diagnostics | Propagate parser errors; no local grammar or diagnostic replacement |
| BLOB helper APIs, NullsOrder traits/helpers | No new syntax data; existing representation remains sufficient |

Final regressions exercise real SQL through the owned boundary. Baseline-only
UPDATE ORDER BY/LIMIT cases remain visible in the pre-upgrade repair commit;
the final tree tests their rejection under 0.8.1. No semantic/codegen feature
is inferred from successful syntax adaptation.
