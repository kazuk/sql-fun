use crate::sem::unresolved::postgres::{nonempty, required};
use crate::sem::unresolved::{
    Access, Alias, Assignment, CommitAction, Conflict, ConflictAction, Cte, CteBody, Cycle, Delete,
    DerivedTable, Expr, Frame, FrameBound, FrameExclusion, FrameMode, FromItem, Grouping,
    IndexExpression, Insert, Into, Join, JoinKind, Lock, LockStrength, LockWait, Name, Order,
    OverridingKind, Query, QueryBody, Relation, Search, Select, SetOperation, SetQuery,
    SyntaxOption, SyntaxValue, TableFunction, TableFunctions, TableSample, Target, TypeName,
    TypedColumn, Update, Window, With,
};
use crate::syn::{ListOpt, Opt};
use crate::{sem::AnalysisError, syn};

use ::sql_fun_core::IVec;

impl Query {
    pub(crate) fn postgres(
        value: &syn::SelectStmt,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        Ok(Self {
            with: With::postgres(value.get_with_clause(), tokens)?,
            body: QueryBody::postgres(value, tokens)?,
            order: Order::postgres_list(value.get_sort_clause(), tokens)?,
            limit: Expr::postgres_optional(value.get_limit_count(), tokens)?,
            offset: Expr::postgres_optional(value.get_limit_offset(), tokens)?,
            with_ties: matches!(
                value.get_limit_option().as_inner(),
                Some(syn::LimitOption::WithTies)
            ),
            locks: Lock::postgres_list(value.get_locking_clause())?,
        })
    }
}

impl QueryBody {
    pub(in crate::sem::unresolved) fn postgres(
        value: &syn::SelectStmt,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        if let Some(operation) = SetOperation::postgres(value.get_op().as_inner())? {
            return Ok(Self::Set(Box::new(SetQuery {
                operation,
                all: value.get_all(),
                left: Query::postgres(&required(value.get_larg().as_inner(), "set left")?, tokens)?,
                right: Query::postgres(
                    &required(value.get_rarg().as_inner(), "set right")?,
                    tokens,
                )?,
            })));
        }
        let rows = value.get_values_lists().as_inner().unwrap_or_default();
        if !rows.is_empty() {
            let mut values = Vec::new();
            for row in rows {
                values.push(Expr::postgres_list(
                    required(row.as_list().as_inner(), "values row")?.get_items(),
                    tokens,
                )?);
            }
            return Ok(Self::Values(values));
        }
        let distinct = value.get_distinct_clause().as_inner().unwrap_or_default();
        let distinct = if distinct.is_empty() {
            None
        } else {
            let mut expressions = Vec::new();
            for expression in distinct {
                if expression.as_inner().is_some() {
                    expressions.push(Expr::postgres(&expression, tokens)?);
                }
            }
            Some(expressions)
        };
        let mut windows = Vec::new();
        for window in value.get_window_clause().as_inner().unwrap_or_default() {
            windows.push(Window::postgres(
                &required(window.as_window_def().as_inner(), "window")?,
                tokens,
            )?);
        }
        Ok(Self::Select(Box::new(Select {
            into: value
                .get_into_clause()
                .as_inner()
                .map(|into| Into::postgres(&into, tokens))
                .transpose()?,
            distinct,
            targets: Target::postgres_list(value.get_target_list(), tokens)?,
            from: FromItem::postgres_list(value.get_from_clause(), tokens)?,
            predicate: Expr::postgres_optional(value.get_where_clause(), tokens)?,
            group: Grouping::postgres_list(value.get_group_clause(), tokens)?,
            group_distinct: value.get_group_distinct(),
            having: Expr::postgres_optional(value.get_having_clause(), tokens)?,
            windows,
        })))
    }
}

impl SetOperation {
    pub(in crate::sem::unresolved) fn postgres(
        operation: Option<syn::SetOperation>,
    ) -> Result<Option<Self>, AnalysisError> {
        match operation {
            Some(syn::SetOperation::SetopNone) => Ok(None),
            Some(syn::SetOperation::SetopUnion) => Ok(Some(Self::Union)),
            Some(syn::SetOperation::SetopIntersect) => Ok(Some(Self::Intersect)),
            Some(syn::SetOperation::SetopExcept) => Ok(Some(Self::Except)),
            _ => AnalysisError::raise_unexpected_input("set operation"),
        }
    }
}

impl Target {
    pub(in crate::sem::unresolved) fn postgres_list(
        nodes: syn::NodeListOpt,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Vec<Self>, AnalysisError> {
        let mut targets = Vec::new();
        for node in nodes.as_inner().unwrap_or_default() {
            let target = required(node.as_res_target().as_inner(), "query target")?;
            targets.push(Self {
                expression: Expr::postgres(&target.get_val(), tokens)?,
                alias: nonempty(target.get_name()),
            });
        }
        Ok(targets)
    }
}

impl Order {
    pub(in crate::sem::unresolved) fn postgres_list(
        nodes: syn::NodeListOpt,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Vec<Self>, AnalysisError> {
        let mut orders = Vec::new();
        for node in nodes.as_inner().unwrap_or_default() {
            let order = required(node.as_sort_by().as_inner(), "sort expression")?;
            let using = Name::postgres(order.get_use_op())?;
            orders.push(Self {
                expression: Expr::postgres(&order.get_node(), tokens)?,
                descending: sort_direction(order.get_sortby_dir().as_inner()),
                nulls_first: null_order(order.get_sortby_nulls().as_inner()),
                using: if using.0.is_empty() {
                    None
                } else {
                    Some(using)
                },
            });
        }
        Ok(orders)
    }
}

const fn sort_direction(value: Option<syn::SortByDir>) -> Option<bool> {
    match value {
        Some(syn::SortByDir::SortbyAsc) => Some(false),
        Some(syn::SortByDir::SortbyDesc) => Some(true),
        _ => None,
    }
}

const fn null_order(value: Option<syn::SortByNulls>) -> Option<bool> {
    match value {
        Some(syn::SortByNulls::SortbyNullsFirst) => Some(true),
        Some(syn::SortByNulls::SortbyNullsLast) => Some(false),
        _ => None,
    }
}

impl Grouping {
    pub(in crate::sem::unresolved) fn postgres_list(
        nodes: syn::NodeListOpt,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Vec<Self>, AnalysisError> {
        let mut groups = Vec::new();
        for node in nodes.as_inner().unwrap_or_default() {
            groups.push(if let Some(group) = node.as_grouping_set().as_inner() {
                let content = Self::postgres_list(group.get_content(), tokens)?;
                match required(group.get_kind().as_inner(), "grouping kind")? {
                    syn::GroupingSetKind::GroupingSetEmpty => Self::Empty,
                    syn::GroupingSetKind::GroupingSetSimple => Self::Simple(content),
                    syn::GroupingSetKind::GroupingSetRollup => Self::Rollup(content),
                    syn::GroupingSetKind::GroupingSetCube => Self::Cube(content),
                    syn::GroupingSetKind::GroupingSetSets => Self::Sets(content),
                    _ => return AnalysisError::raise_unexpected_input("grouping kind"),
                }
            } else {
                Self::Expression(Expr::postgres(&node, tokens)?)
            });
        }
        Ok(groups)
    }
}

impl With {
    pub(crate) fn postgres(
        value: syn::WithClauseOpt,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        let Some(value) = value.as_inner() else {
            return Ok(Self::default());
        };
        let mut entries = Vec::new();
        for entry in value.get_ctes().as_inner().unwrap_or_default() {
            entries.push(Cte::postgres(
                &required(entry.as_common_table_expr().as_inner(), "CTE")?,
                tokens,
            )?);
        }
        Ok(Self {
            recursive: value.get_recursive(),
            entries,
        })
    }
}

impl Cte {
    pub(in crate::sem::unresolved) fn postgres(
        value: &syn::CommonTableExpr,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        Ok(Self {
            name: value.get_ctename(),
            columns: crate::sem::unresolved::CteColumn::from_names(
                &Name::postgres(value.get_aliascolnames())?.0,
            ),
            materialized: match value.get_ctematerialized().as_inner() {
                Some(syn::CteMaterialize::Always) => Some(true),
                Some(syn::CteMaterialize::Never) => Some(false),
                _ => None,
            },
            body: CteBody::postgres(&value.get_ctequery(), tokens)?,
            search: value
                .get_search_clause()
                .as_inner()
                .map(|search| Search::postgres(&search))
                .transpose()?,
            cycle: value
                .get_cycle_clause()
                .as_inner()
                .map(|cycle| Cycle::postgres(&cycle, tokens))
                .transpose()?,
        })
    }
}

impl CteBody {
    pub(in crate::sem::unresolved) fn postgres(
        value: &syn::Node,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        if let Some(query) = value.as_select_stmt().as_inner() {
            return Ok(Self::Query(Box::new(Query::postgres(&query, tokens)?)));
        }
        if let Some(insert) = value.as_insert_stmt().as_inner() {
            return Ok(Self::Insert(Box::new(Insert::postgres(&insert, tokens)?)));
        }
        if let Some(update) = value.as_update_stmt().as_inner() {
            return Ok(Self::Update(Box::new(Update::postgres(&update, tokens)?)));
        }
        if let Some(delete) = value.as_delete_stmt().as_inner() {
            return Ok(Self::Delete(Box::new(Delete::postgres(&delete, tokens)?)));
        }
        if let Some(merge) = value.as_merge_stmt().as_inner() {
            return Ok(Self::Merge(Box::new(
                crate::sem::unresolved::Merge::postgres(&merge, tokens)?,
            )));
        }
        AnalysisError::raise_unexpected_input("CTE body")
    }
}

impl Search {
    pub(in crate::sem::unresolved) fn postgres(
        value: &syn::CteSearchClause,
    ) -> Result<Self, AnalysisError> {
        Ok(Self {
            columns: Name::postgres(value.get_search_col_list())?.0,
            breadth_first: value.get_search_breadth_first(),
            sequence: value.get_search_seq_column(),
        })
    }
}

impl Cycle {
    pub(in crate::sem::unresolved) fn postgres(
        value: &syn::CteCycleClause,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        Ok(Self {
            columns: Name::postgres(value.get_cycle_col_list())?.0,
            mark: value.get_cycle_mark_column(),
            value: Expr::postgres_optional(value.get_cycle_mark_value(), tokens)?,
            default: Expr::postgres_optional(value.get_cycle_mark_default(), tokens)?,
            path: value.get_cycle_path_column(),
        })
    }
}

impl Alias {
    pub(in crate::sem::unresolved) fn postgres(
        value: syn::AliasOpt,
    ) -> Result<Option<Self>, AnalysisError> {
        value
            .as_inner()
            .map(|alias| {
                Ok(Self {
                    name: alias.get_aliasname(),
                    columns: Name::postgres(alias.get_colnames())?.0,
                })
            })
            .transpose()
    }
}

impl Relation {
    pub(in crate::sem::unresolved) fn postgres(
        value: &syn::RangeVar,
    ) -> Result<Self, AnalysisError> {
        let mut parts = Vec::new();
        if let Some(catalog) = nonempty(value.get_catalogname()) {
            parts.push(catalog);
        }
        if let Some(schema) = nonempty(value.get_schemaname()) {
            parts.push(schema);
        }
        parts.push(value.get_relname());
        Ok(Self {
            name: Name::from_parts(&parts),
            alias: Alias::postgres(value.get_alias())?,
            inherit: value.get_inh(),
            persistence: value.get_relpersistence(),
            indexed: None,
            not_indexed: false,
        })
    }
}

impl FromItem {
    pub(crate) fn postgres_list(
        nodes: syn::NodeListOpt,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Vec<Self>, AnalysisError> {
        nodes
            .as_inner()
            .unwrap_or_default()
            .into_iter()
            .map(|node| Self::postgres(&node, tokens))
            .collect()
    }

    pub(crate) fn postgres(
        node: &syn::Node,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        if let Some(relation) = node.as_range_var().as_inner() {
            return Self::postgres_range_var(&relation, tokens);
        }
        if let Some(query) = node.as_range_subselect().as_inner() {
            return Self::postgres_range_subselect(&query, tokens);
        }
        if let Some(join) = node.as_join_expr().as_inner() {
            return Self::postgres_join_expr(&join, tokens);
        }
        if let Some(functions) = node.as_range_function().as_inner() {
            return Self::postgres_range_function(&functions, tokens);
        }
        if let Some(sample) = node.as_range_table_sample().as_inner() {
            return Ok(Self::Sample(Box::new(TableSample {
                relation: Self::postgres(&sample.get_relation(), tokens)?,
                method: Name::postgres(sample.get_method())?,
                arguments: Expr::postgres_list(sample.get_args(), tokens)?,
                repeatable: Expr::postgres_optional(sample.get_repeatable(), tokens)?,
            })));
        }
        if let Some(value) = node.as_range_table_func().as_inner() {
            return Self::postgres_range_table_func(&value, tokens);
        }
        if let Some(value) = node.as_json_table().as_inner() {
            return Ok(Self::Json(Box::new(
                crate::sem::unresolved::JsonTable::postgres(&value, tokens)?,
            )));
        }
        AnalysisError::raise_unexpected_input(&format!("FROM syntax: {node:?}"))
    }
}

impl JoinKind {
    pub(in crate::sem::unresolved) fn postgres(
        value: syn::JoinType,
    ) -> Result<Self, AnalysisError> {
        match value {
            syn::JoinType::JoinInner => Ok(Self::Inner),
            syn::JoinType::JoinLeft => Ok(Self::Left),
            syn::JoinType::JoinRight => Ok(Self::Right),
            syn::JoinType::JoinFull => Ok(Self::Full),
            _ => AnalysisError::raise_unexpected_input("join kind"),
        }
    }
}

impl TableFunctions {
    pub(in crate::sem::unresolved) fn postgres(
        value: &syn::RangeFunction,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        let mut calls = Vec::new();
        for node in value.get_functions().as_inner().unwrap_or_default() {
            if node.is_func_call() || node.is_sqlvalue_function() {
                calls.push(TableFunction {
                    expression: Expr::postgres(&node, tokens)?,
                    columns: Vec::new(),
                });
                continue;
            }
            let items = required(node.as_list().as_inner(), "table function pair")?.get_items();
            let expression = Expr::postgres(&items.get(0), tokens)?;
            let columns = if let Some(columns) = items.get(1).as_list().as_inner() {
                TypedColumn::postgres_list(columns.get_items(), tokens)?
            } else {
                Vec::new()
            };
            calls.push(TableFunction {
                expression,
                columns,
            });
        }
        Ok(Self {
            calls,
            alias: Alias::postgres(value.get_alias())?,
            lateral: value.get_lateral(),
            ordinality: value.get_ordinality(),
            rows_from: value.get_is_rowsfrom(),
            columns: TypedColumn::postgres_list(value.get_coldeflist(), tokens)?,
        })
    }
}

impl TypedColumn {
    pub(in crate::sem::unresolved) fn postgres_list(
        nodes: syn::NodeListOpt,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Vec<Self>, AnalysisError> {
        let mut columns = Vec::new();
        for node in nodes.as_inner().unwrap_or_default() {
            let column = required(node.as_column_def().as_inner(), "table function column")?;
            columns.push(Self {
                name: column.get_colname(),
                type_name: TypeName::postgres(
                    &required(column.get_type_name().as_inner(), "column type")?,
                    tokens,
                )?,
                collation: column
                    .get_coll_clause()
                    .as_inner()
                    .map(|collation| Name::postgres(collation.get_collname()))
                    .transpose()?,
            });
        }
        Ok(columns)
    }
}

impl Window {
    pub(in crate::sem::unresolved) fn postgres(
        value: &syn::WindowDef,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        Ok(Self {
            name: nonempty(value.get_name()),
            reference: nonempty(value.get_refname()),
            partition: Expr::postgres_list(value.get_partition_clause(), tokens)?,
            order: Order::postgres_list(value.get_order_clause(), tokens)?,
            frame: Frame::postgres(value, tokens)?,
        })
    }
}

impl Frame {
    pub(in crate::sem::unresolved) fn postgres(
        value: &syn::WindowDef,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Option<Self>, AnalysisError> {
        // Decode the parser's flags at the boundary; the common AST stores SQL concepts.
        let flags = value.get_frame_options();
        if flags & 0x00001 == 0 {
            return Ok(None);
        }
        let mode = FrameMode::postgres(flags);
        let start = FrameBound::postgres(
            flags,
            true,
            Expr::postgres_optional(value.get_start_offset(), tokens)?,
        )?;
        let end = if flags & 0x00010 != 0 {
            Some(FrameBound::postgres(
                flags,
                false,
                Expr::postgres_optional(value.get_end_offset(), tokens)?,
            )?)
        } else {
            None
        };
        Ok(Some(Self {
            mode,
            start,
            end,
            exclusion: FrameExclusion::postgres(flags),
        }))
    }
}

impl FrameMode {
    pub(in crate::sem::unresolved) const fn postgres(flags: i32) -> Self {
        if flags & 0x00004 != 0 {
            Self::Rows
        } else if flags & 0x00008 != 0 {
            Self::Groups
        } else {
            Self::Range
        }
    }
}

impl FrameBound {
    pub(in crate::sem::unresolved) fn postgres(
        flags: i32,
        start: bool,
        offset: Option<Expr>,
    ) -> Result<Self, AnalysisError> {
        let flags = if start { flags } else { flags >> 1 };
        if flags & 0x00020 != 0 {
            Ok(Self::UnboundedPreceding)
        } else if flags & 0x00080 != 0 {
            Ok(Self::UnboundedFollowing)
        } else if flags & 0x00200 != 0 {
            Ok(Self::CurrentRow)
        } else if flags & 0x00800 != 0 {
            Ok(Self::Preceding(required(offset, "frame preceding offset")?))
        } else if flags & 0x02000 != 0 {
            Ok(Self::Following(required(offset, "frame following offset")?))
        } else {
            AnalysisError::raise_unexpected_input("frame bound")
        }
    }
}

impl FrameExclusion {
    pub(in crate::sem::unresolved) const fn postgres(flags: i32) -> Option<Self> {
        if flags & 0x08000 != 0 {
            Some(Self::CurrentRow)
        } else if flags & 0x10000 != 0 {
            Some(Self::Group)
        } else if flags & 0x20000 != 0 {
            Some(Self::Ties)
        } else {
            None
        }
    }
}

impl Lock {
    pub(in crate::sem::unresolved) fn postgres_list(
        nodes: syn::NodeListOpt,
    ) -> Result<Vec<Self>, AnalysisError> {
        let mut locks = Vec::new();
        for node in nodes.as_inner().unwrap_or_default() {
            let lock = required(node.as_locking_clause().as_inner(), "locking clause")?;
            let mut relations = Vec::new();
            for relation in lock.get_locked_rels().as_inner().unwrap_or_default() {
                relations.push(
                    Relation::postgres(&required(
                        relation.as_range_var().as_inner(),
                        "locked relation",
                    )?)?
                    .name,
                );
            }
            locks.push(Self {
                relations,
                strength: lock
                    .get_strength()
                    .as_inner()
                    .map(LockStrength::postgres)
                    .transpose()?,
                wait: lock
                    .get_wait_policy()
                    .as_inner()
                    .map(LockWait::postgres)
                    .transpose()?,
            });
        }
        Ok(locks)
    }
}

impl Assignment {
    pub(in crate::sem::unresolved) fn postgres_list(
        nodes: syn::NodeListOpt,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Vec<Self>, AnalysisError> {
        let mut assignments = Vec::new();
        for node in nodes.as_inner().unwrap_or_default() {
            let value = required(node.as_res_target().as_inner(), "assignment")?;
            assignments.push(Self {
                name: value.get_name(),
                indirection: Access::postgres_list(value.get_indirection(), tokens)?,
                value: Expr::postgres_optional(value.get_val(), tokens)?,
            });
        }
        Ok(assignments)
    }
}

impl Insert {
    pub(in crate::sem::unresolved) fn postgres(
        value: &syn::InsertStmt,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        Ok(Self {
            with: With::postgres(value.get_with_clause(), tokens)?,
            relation: Relation::postgres(&required(
                value.get_relation().as_inner(),
                "insert relation",
            )?)?,
            columns: Assignment::postgres_list(value.get_cols(), tokens)?,
            source: value
                .get_select_stmt()
                .as_inner()
                .map(|query| {
                    Query::postgres(
                        &required(query.as_select_stmt().as_inner(), "insert query")?,
                        tokens,
                    )
                })
                .transpose()?,
            overriding: value
                .get_override()
                .as_inner()
                .map(OverridingKind::postgres)
                .transpose()?,
            conflict: value
                .get_on_conflict_clause()
                .as_inner()
                .map(|conflict| Conflict::postgres(&conflict, tokens))
                .transpose()?,
            returning: Target::postgres_list(value.get_returning_list(), tokens)?,
        })
    }
}

impl Update {
    pub(in crate::sem::unresolved) fn postgres(
        value: &syn::UpdateStmt,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        Ok(Self {
            with: With::postgres(value.get_with_clause(), tokens)?,
            relation: Relation::postgres(&required(
                value.get_relation().as_inner(),
                "update relation",
            )?)?,
            assignments: Assignment::postgres_list(value.get_target_list(), tokens)?,
            from: FromItem::postgres_list(value.get_from_clause(), tokens)?,
            predicate: Expr::postgres_optional(value.get_where_clause(), tokens)?,
            returning: Target::postgres_list(value.get_returning_list(), tokens)?,
        })
    }
}

impl Delete {
    pub(in crate::sem::unresolved) fn postgres(
        value: &syn::DeleteStmt,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        Ok(Self {
            with: With::postgres(value.get_with_clause(), tokens)?,
            relation: Relation::postgres(&required(
                value.get_relation().as_inner(),
                "delete relation",
            )?)?,
            using: FromItem::postgres_list(value.get_using_clause(), tokens)?,
            predicate: Expr::postgres_optional(value.get_where_clause(), tokens)?,
            returning: Target::postgres_list(value.get_returning_list(), tokens)?,
        })
    }
}

impl Conflict {
    pub(in crate::sem::unresolved) fn postgres(
        value: &syn::OnConflictClause,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        let mut inference = Vec::new();
        let mut inference_predicate = None;
        let mut constraint = None;
        if let Some(infer) = value.get_infer().as_inner() {
            for node in infer.get_index_elems().as_inner().unwrap_or_default() {
                inference.push(IndexExpression::postgres(
                    &required(node.as_index_elem().as_inner(), "conflict index")?,
                    tokens,
                )?);
            }
            inference_predicate = Expr::postgres_optional(infer.get_where_clause(), tokens)?;
            constraint = nonempty(infer.get_conname());
        }
        Ok(Self {
            action: value
                .get_action()
                .as_inner()
                .map(ConflictAction::postgres)
                .transpose()?,
            inference,
            inference_predicate,
            constraint,
            assignments: Assignment::postgres_list(value.get_target_list(), tokens)?,
            predicate: Expr::postgres_optional(value.get_where_clause(), tokens)?,
        })
    }
}

impl IndexExpression {
    pub(in crate::sem::unresolved) fn postgres(
        value: &syn::IndexElem,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        Ok(Self {
            name: nonempty(value.get_name()),
            expression: Expr::postgres_optional(value.get_expr(), tokens)?,
            collation: Name::postgres(value.get_collation())?,
            operator_class: Name::postgres(value.get_opclass())?,
            options: SyntaxOption::postgres_list(value.get_opclassopts(), tokens)?,
            descending: sort_direction(value.get_ordering().as_inner()),
            nulls_first: null_order(value.get_nulls_ordering().as_inner()),
        })
    }
}

impl Into {
    pub(in crate::sem::unresolved) fn postgres(
        value: &syn::IntoClause,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        Ok(Self {
            relation: Relation::postgres(&required(value.get_rel().as_inner(), "INTO relation")?)?,
            columns: Name::postgres(value.get_col_names())?.0,
            access_method: nonempty(value.get_access_method()),
            options: SyntaxOption::postgres_list(value.get_options(), tokens)?,
            on_commit: value
                .get_on_commit()
                .as_inner()
                .map(CommitAction::postgres)
                .transpose()?,
            tablespace: nonempty(value.get_table_space_name()),
            skip_data: value.get_skip_data(),
        })
    }
}

impl SyntaxOption {
    pub(in crate::sem::unresolved) fn postgres_list(
        nodes: syn::NodeListOpt,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Vec<Self>, AnalysisError> {
        let mut options = Vec::new();
        for node in nodes.as_inner().unwrap_or_default() {
            let option = required(node.as_def_elem().as_inner(), "syntax option")?;
            options.push(Self {
                namespace: nonempty(option.get_defnamespace()),
                name: option.get_defname(),
                value: option
                    .get_arg()
                    .as_inner()
                    .map(|node| SyntaxValue::postgres(&node, tokens))
                    .transpose()?,
            });
        }
        Ok(options)
    }
}

impl SyntaxValue {
    fn postgres(node: &syn::Node, tokens: &IVec<syn::ScanToken>) -> Result<Self, AnalysisError> {
        if let Some(value) = node.as_type_name().as_inner() {
            Ok(Self::Type(TypeName::postgres(&value, tokens)?))
        } else {
            Ok(Self::Expression(Expr::postgres(node, tokens)?))
        }
    }
}

impl LockStrength {
    fn postgres(value: syn::LockClauseStrength) -> Result<Self, AnalysisError> {
        match value {
            syn::LockClauseStrength::LcsNone => Ok(Self::None),
            syn::LockClauseStrength::LcsForkeyshare => Ok(Self::KeyShare),
            syn::LockClauseStrength::LcsForshare => Ok(Self::Share),
            syn::LockClauseStrength::LcsFornokeyupdate => Ok(Self::NoKeyUpdate),
            syn::LockClauseStrength::LcsForupdate => Ok(Self::Update),
            _ => AnalysisError::raise_unexpected_input("LockStrength"),
        }
    }
}

impl LockWait {
    fn postgres(value: syn::LockWaitPolicy) -> Result<Self, AnalysisError> {
        match value {
            syn::LockWaitPolicy::LockWaitBlock => Ok(Self::Block),
            syn::LockWaitPolicy::LockWaitSkip => Ok(Self::Skip),
            syn::LockWaitPolicy::LockWaitError => Ok(Self::Error),
            _ => AnalysisError::raise_unexpected_input("LockWait"),
        }
    }
}

impl ConflictAction {
    fn postgres(value: syn::OnConflictAction) -> Result<Self, AnalysisError> {
        match value {
            syn::OnConflictAction::OnconflictNone => Ok(Self::None),
            syn::OnConflictAction::OnconflictNothing => Ok(Self::Nothing),
            syn::OnConflictAction::OnconflictUpdate => Ok(Self::Update),
            _ => AnalysisError::raise_unexpected_input("ConflictAction"),
        }
    }
}

impl CommitAction {
    fn postgres(value: syn::OnCommitAction) -> Result<Self, AnalysisError> {
        match value {
            syn::OnCommitAction::OncommitNoop => Ok(Self::None),
            syn::OnCommitAction::OncommitPreserveRows => Ok(Self::PreserveRows),
            syn::OnCommitAction::OncommitDeleteRows => Ok(Self::DeleteRows),
            syn::OnCommitAction::OncommitDrop => Ok(Self::Drop),
            _ => AnalysisError::raise_unexpected_input("CommitAction"),
        }
    }
}

impl FromItem {
    pub(crate) fn postgres_range_var(
        relation: &syn::RangeVar,
        _tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        Ok(Self::Relation(Relation::postgres(relation)?))
    }

    pub(crate) fn postgres_range_subselect(
        query: &syn::RangeSubselect,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        Ok(Self::Query(Box::new(DerivedTable {
            query: Query::postgres(
                &required(
                    query.get_subquery().as_select_stmt().as_inner(),
                    "derived query",
                )?,
                tokens,
            )?,
            alias: Alias::postgres(query.get_alias())?,
            lateral: query.get_lateral(),
        })))
    }

    pub(crate) fn postgres_join_expr(
        join: &syn::JoinExpr,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        Ok(Self::Join(Box::new(Join {
            left: Self::postgres(&join.get_larg(), tokens)?,
            right: Self::postgres(&join.get_rarg(), tokens)?,
            kind: JoinKind::postgres(required(join.get_jointype().as_inner(), "join kind")?)?,
            natural: join.get_is_natural(),
            condition: Expr::postgres_optional(join.get_quals(), tokens)?,
            using: Name::postgres(join.get_using_clause())?.0,
            using_alias: Alias::postgres(join.get_join_using_alias())?,
            alias: Alias::postgres(join.get_alias())?,
        })))
    }

    pub(crate) fn postgres_range_function(
        functions: &syn::RangeFunction,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        Ok(Self::Functions(TableFunctions::postgres(
            functions, tokens,
        )?))
    }

    pub(crate) fn postgres_range_table_func(
        value: &syn::RangeTableFunc,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, AnalysisError> {
        Ok(Self::Xml(Box::new(
            crate::sem::unresolved::XmlTable::postgres(value, tokens)?,
        )))
    }
}
