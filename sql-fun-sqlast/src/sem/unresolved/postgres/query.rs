use crate::sem::unresolved::postgres::{nonempty, required};
use crate::sem::unresolved::{
    Access, Alias, Assignment, CommitAction, Conflict, ConflictAction, Cte, CteBody, Cycle, Delete,
    DerivedTable, Expr, Frame, FrameBound, FrameExclusion, FrameMode, FromItem, Grouping,
    IndexExpression, Insert, Into, Join, JoinKind, JsonTable, Lock, LockStrength, LockWait, Merge,
    Name, Order, OverridingKind, Query, QueryBody, Relation, Search, Select, SetOperation,
    SetQuery, SyntaxOption, SyntaxValue, TableFunction, TableFunctions, TableSample, Target,
    TryCollectFromWithTokens, TryFromOpt, TryFromWithTokens, TryFromWithTokensOpt, TypeName,
    TypedColumn, Update, Window, With, XmlTable,
};
use crate::syn::{ListOpt, Opt};
use crate::{sem::AnalysisError, syn};

use ::sql_fun_core::IVec;

impl TryFromWithTokens<&syn::SelectStmt> for Query {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::SelectStmt,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            with: With::try_from_with_tokens(value.get_with_clause(), tokens)?,
            body: QueryBody::try_from_with_tokens(value, tokens)?,
            order: Order::try_collect_from_with_tokens(value.get_sort_clause(), tokens)?,
            limit: Expr::try_from_with_tokens_opt(value.get_limit_count(), tokens)?,
            offset: Expr::try_from_with_tokens_opt(value.get_limit_offset(), tokens)?,
            with_ties: matches!(
                value.get_limit_option().as_inner(),
                Some(syn::LimitOption::WithTies)
            ),
            locks: Lock::postgres_list(value.get_locking_clause())?,
        })
    }
}

impl TryFromWithTokens<(SetOperation, &syn::SelectStmt)> for SetQuery {
    type Error = AnalysisError;
    fn try_from_with_tokens(
        value: (SetOperation, &syn::SelectStmt),
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        let operation = value.0;
        let value = value.1;
        Ok(Self {
            operation,
            all: value.get_all(),
            left: Query::try_from_with_tokens(
                &required(value.get_larg().as_inner(), "set left")?,
                tokens,
            )?,
            right: Query::try_from_with_tokens(
                &required(value.get_rarg().as_inner(), "set right")?,
                tokens,
            )?,
        })
    }
}

impl TryFromWithTokens<Vec<syn::Node>> for QueryBody {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        rows: Vec<syn::Node>,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        let mut values = Vec::new();
        for row in rows {
            values.push(Expr::try_collect_from_with_tokens(
                required(row.as_list().as_inner(), "values row")?.get_items(),
                tokens,
            )?);
        }
        Ok(Self::Values(values))
    }
}

impl TryCollectFromWithTokens<syn::NodeListOpt> for Window {
    type Error = AnalysisError;

    fn try_collect_from_with_tokens(
        nodes: syn::NodeListOpt,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Vec<Self>, Self::Error> {
        let mut windows = Vec::new();
        for window in nodes.as_inner().unwrap_or_default() {
            windows.push(Self::try_from_with_tokens(
                &required(window.as_window_def().as_inner(), "window")?,
                tokens,
            )?);
        }
        Ok(windows)
    }
}

impl TryFromWithTokens<&syn::SelectStmt> for QueryBody {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::SelectStmt,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        if let Some(operation) = SetOperation::postgres(value.get_op().as_inner())? {
            return Ok(Self::Set(Box::new(SetQuery::try_from_with_tokens(
                (operation, value),
                tokens,
            )?)));
        }
        let rows = value.get_values_lists().as_inner().unwrap_or_default();
        if !rows.is_empty() {
            return Self::try_from_with_tokens(rows, tokens);
        }
        let distinct = value.get_distinct_clause().as_inner().unwrap_or_default();
        let distinct = if distinct.is_empty() {
            None
        } else {
            let mut expressions = Vec::new();
            for expression in distinct {
                if expression.as_inner().is_some() {
                    expressions.push(Expr::try_from_with_tokens(&expression, tokens)?);
                }
            }
            Some(expressions)
        };

        let windows = Window::try_collect_from_with_tokens(value.get_window_clause(), tokens)?;
        Ok(Self::Select(Box::new(Select {
            into: value
                .get_into_clause()
                .as_inner()
                .map(|into| Into::try_from_with_tokens(&into, tokens))
                .transpose()?,
            distinct,
            targets: Target::try_collect_from_with_tokens(value.get_target_list(), tokens)?,
            from: FromItem::try_collect_from_with_tokens(value.get_from_clause(), tokens)?,
            predicate: Expr::try_from_with_tokens_opt(value.get_where_clause(), tokens)?,
            group: Grouping::try_collect_from_with_tokens(value.get_group_clause(), tokens)?,
            group_distinct: value.get_group_distinct(),
            having: Expr::try_from_with_tokens_opt(value.get_having_clause(), tokens)?,
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

impl TryCollectFromWithTokens<syn::NodeListOpt> for Target {
    type Error = AnalysisError;

    fn try_collect_from_with_tokens(
        nodes: syn::NodeListOpt,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Vec<Self>, Self::Error> {
        let mut targets = Vec::new();
        for node in nodes.as_inner().unwrap_or_default() {
            let target = required(node.as_res_target().as_inner(), "query target")?;
            targets.push(Self {
                expression: Expr::try_from_with_tokens(&target.get_val(), tokens)?,
                alias: nonempty(target.get_name()),
            });
        }
        Ok(targets)
    }
}

impl TryCollectFromWithTokens<syn::NodeListOpt> for Order {
    type Error = AnalysisError;

    fn try_collect_from_with_tokens(
        nodes: syn::NodeListOpt,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Vec<Self>, Self::Error> {
        let mut orders = Vec::new();
        for node in nodes.as_inner().unwrap_or_default() {
            let order = required(node.as_sort_by().as_inner(), "sort expression")?;
            let using = Name::try_from(order.get_use_op())?;
            orders.push(Self {
                expression: Expr::try_from_with_tokens(&order.get_node(), tokens)?,
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

impl TryCollectFromWithTokens<syn::NodeListOpt> for Grouping {
    type Error = AnalysisError;

    fn try_collect_from_with_tokens(
        nodes: syn::NodeListOpt,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Vec<Self>, Self::Error> {
        let mut groups = Vec::new();
        for node in nodes.as_inner().unwrap_or_default() {
            groups.push(if let Some(group) = node.as_grouping_set().as_inner() {
                let content = Self::try_collect_from_with_tokens(group.get_content(), tokens)?;
                match required(group.get_kind().as_inner(), "grouping kind")? {
                    syn::GroupingSetKind::GroupingSetEmpty => Self::Empty,
                    syn::GroupingSetKind::GroupingSetSimple => Self::Simple(content),
                    syn::GroupingSetKind::GroupingSetRollup => Self::Rollup(content),
                    syn::GroupingSetKind::GroupingSetCube => Self::Cube(content),
                    syn::GroupingSetKind::GroupingSetSets => Self::Sets(content),
                    _ => return AnalysisError::raise_unexpected_input("grouping kind"),
                }
            } else {
                Self::Expression(Expr::try_from_with_tokens(&node, tokens)?)
            });
        }
        Ok(groups)
    }
}

impl TryFromWithTokens<syn::WithClauseOpt> for With {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: syn::WithClauseOpt,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        let Some(value) = value.as_inner() else {
            return Ok(Self::default());
        };
        let mut entries = Vec::new();
        for entry in value.get_ctes().as_inner().unwrap_or_default() {
            entries.push(Cte::try_from_with_tokens(
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

impl TryFromWithTokens<&syn::CommonTableExpr> for Cte {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::CommonTableExpr,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            name: value.get_ctename(),
            columns: crate::sem::unresolved::CteColumn::from_names(
                &Name::try_from(value.get_aliascolnames())?.0,
            ),
            materialized: match value.get_ctematerialized().as_inner() {
                Some(syn::CteMaterialize::Always) => Some(true),
                Some(syn::CteMaterialize::Never) => Some(false),
                _ => None,
            },
            body: CteBody::postgres(&value.get_ctequery(), tokens)?,
            search: Search::try_from_opt(value.get_search_clause().as_inner().as_ref())?,
            cycle: value
                .get_cycle_clause()
                .as_inner()
                .map(|cycle| Cycle::try_from_with_tokens(&cycle, tokens))
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
            return Ok(Self::query(Query::try_from_with_tokens(&query, tokens)?));
        }
        if let Some(insert) = value.as_insert_stmt().as_inner() {
            return Ok(Self::insert(Insert::try_from_with_tokens(&insert, tokens)?));
        }
        if let Some(update) = value.as_update_stmt().as_inner() {
            return Ok(Self::update(Update::try_from_with_tokens(&update, tokens)?));
        }
        if let Some(delete) = value.as_delete_stmt().as_inner() {
            return Ok(Self::delete(Delete::try_from_with_tokens(&delete, tokens)?));
        }
        if let Some(merge) = value.as_merge_stmt().as_inner() {
            return Ok(Self::merge(Merge::try_from_with_tokens(&merge, tokens)?));
        }
        AnalysisError::raise_unexpected_input("CTE body")
    }
}

impl TryFrom<&syn::CteSearchClause> for Search {
    type Error = AnalysisError;

    fn try_from(value: &syn::CteSearchClause) -> Result<Self, Self::Error> {
        Ok(Self {
            columns: Name::try_from(value.get_search_col_list())?.0,
            breadth_first: value.get_search_breadth_first(),
            sequence: value.get_search_seq_column(),
        })
    }
}

impl TryFromWithTokens<&syn::CteCycleClause> for Cycle {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::CteCycleClause,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            columns: Name::try_from(value.get_cycle_col_list())?.0,
            mark: value.get_cycle_mark_column(),
            value: Expr::try_from_with_tokens_opt(value.get_cycle_mark_value(), tokens)?,
            default: Expr::try_from_with_tokens_opt(value.get_cycle_mark_default(), tokens)?,
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
                    columns: Name::try_from(alias.get_colnames())?.0,
                })
            })
            .transpose()
    }
}

impl TryFrom<&syn::RangeVar> for Relation {
    type Error = AnalysisError;

    fn try_from(value: &syn::RangeVar) -> Result<Self, Self::Error> {
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

impl TryCollectFromWithTokens<syn::NodeListOpt> for FromItem {
    type Error = AnalysisError;
    fn try_collect_from_with_tokens(
        nodes: syn::NodeListOpt,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Vec<Self>, Self::Error> {
        nodes
            .as_inner()
            .unwrap_or_default()
            .into_iter()
            .map(|node| Self::try_from_with_tokens(&node, tokens))
            .collect()
    }
}

impl TryFromWithTokens<&syn::RangeTableSample> for TableSample {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        sample: &syn::RangeTableSample,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            relation: FromItem::try_from_with_tokens(&sample.get_relation(), tokens)?,
            method: Name::try_from(sample.get_method())?,
            arguments: Expr::try_collect_from_with_tokens(sample.get_args(), tokens)?,
            repeatable: Expr::try_from_with_tokens_opt(sample.get_repeatable(), tokens)?,
        })
    }
}

impl TryFromWithTokens<&syn::Node> for FromItem {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        node: &syn::Node,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        if let Some(relation) = node.as_range_var().as_inner() {
            return Ok(Self::relation(Relation::try_from(&relation)?));
        }
        if let Some(query) = node.as_range_subselect().as_inner() {
            return Ok(Self::query(DerivedTable::try_from_with_tokens(
                &query, tokens,
            )?));
        }
        if let Some(join) = node.as_join_expr().as_inner() {
            return Ok(Self::join(Join::try_from_with_tokens(&join, tokens)?));
        }
        if let Some(functions) = node.as_range_function().as_inner() {
            return Ok(Self::functions(TableFunctions::try_from_with_tokens(
                &functions, tokens,
            )?));
        }
        if let Some(sample) = node.as_range_table_sample().as_inner() {
            return Ok(Self::sample(TableSample::try_from_with_tokens(
                &sample, tokens,
            )?));
        }
        if let Some(value) = node.as_range_table_func().as_inner() {
            return Ok(Self::xml(XmlTable::try_from_with_tokens(&value, tokens)?));
        }
        if let Some(value) = node.as_json_table().as_inner() {
            return Ok(Self::json(JsonTable::try_from_with_tokens(&value, tokens)?));
        }
        AnalysisError::raise_unexpected_input(&format!("FROM syntax: {node:?}"))
    }
}

impl TryFrom<syn::JoinType> for JoinKind {
    type Error = AnalysisError;

    fn try_from(value: syn::JoinType) -> Result<Self, Self::Error> {
        match value {
            syn::JoinType::JoinInner => Ok(Self::Inner),
            syn::JoinType::JoinLeft => Ok(Self::Left),
            syn::JoinType::JoinRight => Ok(Self::Right),
            syn::JoinType::JoinFull => Ok(Self::Full),
            _ => AnalysisError::raise_unexpected_input("join kind"),
        }
    }
}

impl TryFromWithTokens<&syn::RangeFunction> for TableFunctions {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::RangeFunction,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        let mut calls = Vec::new();
        for node in value.get_functions().as_inner().unwrap_or_default() {
            if node.is_func_call() || node.is_sqlvalue_function() {
                calls.push(TableFunction {
                    expression: Expr::try_from_with_tokens(&node, tokens)?,
                    columns: Vec::new(),
                });
                continue;
            }
            let items = required(node.as_list().as_inner(), "table function pair")?.get_items();
            let expression = Expr::try_from_with_tokens(&items.get(0), tokens)?;
            let columns = if let Some(columns) = items.get(1).as_list().as_inner() {
                TypedColumn::try_collect_from_with_tokens(columns.get_items(), tokens)?
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
            columns: TypedColumn::try_collect_from_with_tokens(value.get_coldeflist(), tokens)?,
        })
    }
}

impl TryCollectFromWithTokens<syn::NodeListOpt> for TypedColumn {
    type Error = AnalysisError;

    fn try_collect_from_with_tokens(
        nodes: syn::NodeListOpt,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Vec<Self>, Self::Error> {
        let mut columns = Vec::new();
        for node in nodes.as_inner().unwrap_or_default() {
            let column = required(node.as_column_def().as_inner(), "table function column")?;
            columns.push(Self {
                name: column.get_colname(),
                type_name: TypeName::try_from_with_tokens(
                    &required(column.get_type_name().as_inner(), "column type")?,
                    tokens,
                )?,
                collation: column
                    .get_coll_clause()
                    .as_inner()
                    .map(|collation| Name::try_from(collation.get_collname()))
                    .transpose()?,
            });
        }
        Ok(columns)
    }
}

impl TryFromWithTokens<&syn::WindowDef> for Window {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::WindowDef,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            name: nonempty(value.get_name()),
            reference: nonempty(value.get_refname()),
            partition: Expr::try_collect_from_with_tokens(value.get_partition_clause(), tokens)?,
            order: Order::try_collect_from_with_tokens(value.get_order_clause(), tokens)?,
            frame: Frame::try_from_with_tokens_opt(value, tokens)?,
        })
    }
}

impl TryFromWithTokensOpt<&syn::WindowDef> for Frame {
    type Error = AnalysisError;

    fn try_from_with_tokens_opt(
        value: &syn::WindowDef,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Option<Self>, Self::Error> {
        // Decode the parser's flags at the boundary; the common AST stores SQL concepts.
        let flags = value.get_frame_options();
        if flags & 0x00001 == 0 {
            return Ok(None);
        }
        let mode = FrameMode::postgres(flags);
        let start = FrameBound::postgres(
            flags,
            true,
            Expr::try_from_with_tokens_opt(value.get_start_offset(), tokens)?,
        )?;
        let end = if flags & 0x00010 != 0 {
            Some(FrameBound::postgres(
                flags,
                false,
                Expr::try_from_with_tokens_opt(value.get_end_offset(), tokens)?,
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
                    Relation::try_from(&required(
                        relation.as_range_var().as_inner(),
                        "locked relation",
                    )?)?
                    .name,
                );
            }
            locks.push(Self {
                relations,
                strength: LockStrength::try_from_opt(lock.get_strength().as_inner())?,
                wait: LockWait::try_from_opt(lock.get_wait_policy().as_inner())?,
            });
        }
        Ok(locks)
    }
}

impl TryCollectFromWithTokens<syn::NodeListOpt> for Assignment {
    type Error = AnalysisError;

    fn try_collect_from_with_tokens(
        nodes: syn::NodeListOpt,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Vec<Self>, Self::Error> {
        let mut assignments = Vec::new();
        for node in nodes.as_inner().unwrap_or_default() {
            let value = required(node.as_res_target().as_inner(), "assignment")?;
            assignments.push(Self {
                name: value.get_name(),
                indirection: Access::try_collect_from_with_tokens(value.get_indirection(), tokens)?,
                value: Expr::try_from_with_tokens_opt(value.get_val(), tokens)?,
            });
        }
        Ok(assignments)
    }
}

impl TryFromWithTokens<&syn::InsertStmt> for Insert {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::InsertStmt,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            with: With::try_from_with_tokens(value.get_with_clause(), tokens)?,
            relation: Relation::try_from(&required(
                value.get_relation().as_inner(),
                "insert relation",
            )?)?,
            columns: Assignment::try_collect_from_with_tokens(value.get_cols(), tokens)?,
            source: value
                .get_select_stmt()
                .as_inner()
                .map(|query| {
                    Query::try_from_with_tokens(
                        &required(query.as_select_stmt().as_inner(), "insert query")?,
                        tokens,
                    )
                })
                .transpose()?,
            overriding: OverridingKind::try_from_opt(value.get_override().as_inner())?,
            conflict: value
                .get_on_conflict_clause()
                .as_inner()
                .map(|conflict| Conflict::try_from_with_tokens(&conflict, tokens))
                .transpose()?,
            returning: Target::try_collect_from_with_tokens(value.get_returning_list(), tokens)?,
        })
    }
}

impl TryFromWithTokens<&syn::UpdateStmt> for Update {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::UpdateStmt,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            with: With::try_from_with_tokens(value.get_with_clause(), tokens)?,
            relation: Relation::try_from(&required(
                value.get_relation().as_inner(),
                "update relation",
            )?)?,
            assignments: Assignment::try_collect_from_with_tokens(value.get_target_list(), tokens)?,
            from: FromItem::try_collect_from_with_tokens(value.get_from_clause(), tokens)?,
            predicate: Expr::try_from_with_tokens_opt(value.get_where_clause(), tokens)?,
            returning: Target::try_collect_from_with_tokens(value.get_returning_list(), tokens)?,
        })
    }
}

impl TryFromWithTokens<&syn::DeleteStmt> for Delete {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::DeleteStmt,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            with: With::try_from_with_tokens(value.get_with_clause(), tokens)?,
            relation: Relation::try_from(&required(
                value.get_relation().as_inner(),
                "delete relation",
            )?)?,
            using: FromItem::try_collect_from_with_tokens(value.get_using_clause(), tokens)?,
            predicate: Expr::try_from_with_tokens_opt(value.get_where_clause(), tokens)?,
            returning: Target::try_collect_from_with_tokens(value.get_returning_list(), tokens)?,
        })
    }
}

impl TryFromWithTokens<&syn::OnConflictClause> for Conflict {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::OnConflictClause,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        let mut inference = Vec::new();
        let mut inference_predicate = None;
        let mut constraint = None;
        if let Some(infer) = value.get_infer().as_inner() {
            for node in infer.get_index_elems().as_inner().unwrap_or_default() {
                inference.push(IndexExpression::try_from_with_tokens(
                    &required(node.as_index_elem().as_inner(), "conflict index")?,
                    tokens,
                )?);
            }
            inference_predicate = Expr::try_from_with_tokens_opt(infer.get_where_clause(), tokens)?;
            constraint = nonempty(infer.get_conname());
        }
        Ok(Self {
            action: ConflictAction::try_from_opt(value.get_action().as_inner())?,
            inference,
            inference_predicate,
            constraint,
            assignments: Assignment::try_collect_from_with_tokens(value.get_target_list(), tokens)?,
            predicate: Expr::try_from_with_tokens_opt(value.get_where_clause(), tokens)?,
        })
    }
}

impl TryFromWithTokens<&syn::IndexElem> for IndexExpression {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::IndexElem,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            name: nonempty(value.get_name()),
            alias: nonempty(value.get_indexcolname()),
            expression: Expr::try_from_with_tokens_opt(value.get_expr(), tokens)?,
            collation: Name::try_from(value.get_collation())?,
            operator_class: Name::try_from(value.get_opclass())?,
            options: SyntaxOption::try_collect_from_with_tokens(value.get_opclassopts(), tokens)?,
            descending: sort_direction(value.get_ordering().as_inner()),
            using_order: matches!(
                value.get_ordering().as_inner(),
                Some(syn::SortByDir::SortbyUsing)
            ),
            nulls_first: null_order(value.get_nulls_ordering().as_inner()),
        })
    }
}

impl TryFromWithTokens<&syn::IntoClause> for Into {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::IntoClause,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            relation: Relation::try_from(&required(value.get_rel().as_inner(), "INTO relation")?)?,
            columns: Name::try_from(value.get_col_names())?.0,
            access_method: nonempty(value.get_access_method()),
            options: SyntaxOption::try_collect_from_with_tokens(value.get_options(), tokens)?,
            on_commit: CommitAction::try_from_opt(value.get_on_commit().as_inner())?,
            tablespace: nonempty(value.get_table_space_name()),
            skip_data: value.get_skip_data(),
        })
    }
}

impl TryCollectFromWithTokens<syn::NodeListOpt> for SyntaxOption {
    type Error = AnalysisError;

    fn try_collect_from_with_tokens(
        nodes: syn::NodeListOpt,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Vec<Self>, Self::Error> {
        let mut options = Vec::new();
        for node in nodes.as_inner().unwrap_or_default() {
            let option = required(node.as_def_elem().as_inner(), "syntax option")?;
            options.push(Self {
                namespace: nonempty(option.get_defnamespace()),
                name: option.get_defname(),
                value: option
                    .get_arg()
                    .as_inner()
                    .map(|node| SyntaxValue::try_from_with_tokens(&node, tokens))
                    .transpose()?,
            });
        }
        Ok(options)
    }
}

impl TryFromWithTokens<&syn::Node> for SyntaxValue {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        node: &syn::Node,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        if let Some(value) = node.as_type_name().as_inner() {
            Ok(Self::Type(TypeName::try_from_with_tokens(&value, tokens)?))
        } else {
            Ok(Self::Expression(Expr::try_from_with_tokens(node, tokens)?))
        }
    }
}

impl TryFrom<syn::LockClauseStrength> for LockStrength {
    type Error = AnalysisError;

    fn try_from(value: syn::LockClauseStrength) -> Result<Self, Self::Error> {
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

impl TryFrom<syn::LockWaitPolicy> for LockWait {
    type Error = AnalysisError;

    fn try_from(value: syn::LockWaitPolicy) -> Result<Self, AnalysisError> {
        match value {
            syn::LockWaitPolicy::LockWaitBlock => Ok(Self::Block),
            syn::LockWaitPolicy::LockWaitSkip => Ok(Self::Skip),
            syn::LockWaitPolicy::LockWaitError => Ok(Self::Error),
            _ => AnalysisError::raise_unexpected_input("LockWait"),
        }
    }
}

impl TryFrom<syn::OnConflictAction> for ConflictAction {
    type Error = AnalysisError;

    fn try_from(value: syn::OnConflictAction) -> Result<Self, Self::Error> {
        match value {
            syn::OnConflictAction::OnconflictNone => Ok(Self::None),
            syn::OnConflictAction::OnconflictNothing => Ok(Self::Nothing),
            syn::OnConflictAction::OnconflictUpdate => Ok(Self::Update),
            _ => AnalysisError::raise_unexpected_input("ConflictAction"),
        }
    }
}

impl TryFrom<syn::OnCommitAction> for CommitAction {
    type Error = AnalysisError;

    fn try_from(value: syn::OnCommitAction) -> Result<Self, Self::Error> {
        match value {
            syn::OnCommitAction::OncommitNoop => Ok(Self::None),
            syn::OnCommitAction::OncommitPreserveRows => Ok(Self::PreserveRows),
            syn::OnCommitAction::OncommitDeleteRows => Ok(Self::DeleteRows),
            syn::OnCommitAction::OncommitDrop => Ok(Self::Drop),
            _ => AnalysisError::raise_unexpected_input("CommitAction"),
        }
    }
}

impl TryFromWithTokens<&syn::RangeSubselect> for DerivedTable {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        query: &syn::RangeSubselect,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            query: Query::try_from_with_tokens(
                &required(
                    query.get_subquery().as_select_stmt().as_inner(),
                    "derived query",
                )?,
                tokens,
            )?,
            alias: Alias::postgres(query.get_alias())?,
            lateral: query.get_lateral(),
        })
    }
}

impl TryFromWithTokens<&syn::JoinExpr> for Join {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        join: &syn::JoinExpr,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            left: FromItem::try_from_with_tokens(&join.get_larg(), tokens)?,
            right: FromItem::try_from_with_tokens(&join.get_rarg(), tokens)?,
            kind: JoinKind::try_from(required(join.get_jointype().as_inner(), "join kind")?)?,
            natural: join.get_is_natural(),
            condition: Expr::try_from_with_tokens_opt(join.get_quals(), tokens)?,
            using: Name::try_from(join.get_using_clause())?.0,
            using_alias: Alias::postgres(join.get_join_using_alias())?,
            alias: Alias::postgres(join.get_alias())?,
        })
    }
}
