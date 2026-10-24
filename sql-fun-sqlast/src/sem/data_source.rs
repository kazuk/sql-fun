mod alias;
mod analyze;
mod column_candidates;
mod cte;
mod deferred_fun;
mod function_columns;
mod group;
mod join_expr;
mod join_kind;
mod merge_record;
mod namespace;
mod projection;
mod qualified;
mod resolved;
mod rows_fun;
mod sub_query;
mod table;
mod table_fun;
mod undefined;
mod view;

pub use self::{
    alias::{AliasName, AliasSpec},
    cte::CteDataSource,
    deferred_fun::DeferredTableFunction,
    group::GroupedDataSource,
    join_expr::JoinExprDataSource,
    join_kind::JoinKind,
    merge_record::{MergeRecordDataSource, MergeRecordRole},
    rows_fun::{DataSourceRowFunctions, DataSourceRowsFromFunc},
    sub_query::SubQueryDataSource,
    table::TableDataSource,
    table_fun::TableFunctionDataSource,
    undefined::UndefinedTableFuncDataSource,
    view::ViewDataSource,
};
pub(crate) use self::{
    column_candidates::ColumnCandidates,
    function_columns::TableFunctionOutput,
    join_expr::JoinUsingClause,
    namespace::SourceNamespace,
    projection::{ProjectionColumn, WildcardProjection},
    qualified::QualifiedSource,
};

use crate::sem::{
    AnalysisError, ColumnName, ParseContext, SemScalarExpr, WithClause,
    scalar_expr::ColumnReferenceExpr,
};

/// source nullability type
pub type Nullable = bool;

/// from / join sources
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum DataSource {
    /// from CTE defined set
    CteQuery(CteDataSource),
    /// from a table
    Table(TableDataSource),
    /// from a view
    View(ViewDataSource),
    /// join expression
    JoinExpr(Box<JoinExprDataSource>),
    /// A target or source record exposed by a MERGE scope.
    MergeRecord(Box<MergeRecordDataSource>),
    /// SQLite parenthesized source with a group alias.
    Group(Box<GroupedDataSource>),
    /// sub-query
    SubQuery(Box<SubQueryDataSource>),
    /// table function
    CallTableFunc(TableFunctionDataSource),
    /// Function name lookup is complete; signature and output columns are deferred.
    DeferredTableFunc(DeferredTableFunction),
    /// table function as source but not found
    UndefinedTableFunc(UndefinedTableFuncDataSource),
    /// ROWS clause
    RowFunctions(DataSourceRowFunctions),
    /// range table sampling
    RangeTableSample(), // todo modeling
    /// view or table or CTE not found
    Unknown(String, AliasSpec),
}

impl DataSource {
    pub(crate) fn projection_is_table_dependent(&self, context: &impl ParseContext) -> bool {
        match self {
            Self::Table(_) => true,
            Self::JoinExpr(join) => join.projection_is_table_dependent(context),
            Self::Group(group) => group.projection_is_table_dependent(context),
            Self::MergeRecord(record) => record.source().projection_is_table_dependent(context),
            Self::CallTableFunc(source) => source.function().table_output_depends_on_table(context),
            Self::DeferredTableFunc(source) => source
                .call()
                .overload()
                .as_ref()
                .is_some_and(|overload| overload.table_output_depends_on_table(context)),
            Self::RowFunctions(source) => source
                .functions()
                .iter()
                .any(|function| function.source().projection_is_table_dependent(context)),
            _ => false,
        }
    }

    pub(super) fn create_column_ref<'context, TParseContext>(
        &self,
        context: &'context mut TParseContext,
        with_clause: &WithClause,
        alias: &AliasName,
        column: &ColumnName,
        nullability: bool,
    ) -> Result<(SemScalarExpr, &'context mut TParseContext), AnalysisError>
    where
        TParseContext: crate::sem::ParseContext,
    {
        match self {
            Self::CteQuery(cteds) => {
                let column_ref =
                    cteds.create_column_ref(context, with_clause, alias, column, nullability)?;
                Ok((SemScalarExpr::ColumnRef(column_ref), context))
            }
            Self::Table(tableds) => {
                let column_ref = tableds.create_column_ref(context, alias, column, nullability)?;
                Ok((SemScalarExpr::ColumnRef(column_ref), context))
            }
            Self::View(viewds) => {
                let reference = viewds.create_column_ref(context, alias, column, nullability)?;
                Ok((SemScalarExpr::column_reference(reference), context))
            }
            Self::SubQuery(subqds) => {
                let column_ref = subqds.create_column_ref(context, alias, column, nullability)?;
                Ok((SemScalarExpr::ColumnRef(column_ref), context))
            }
            Self::Unknown(_, _) => {
                // DataSource::Unknown ready report_problem in from_clause analysis
                ColumnReferenceExpr::unknown_source_ref(context, alias, column)
            }
            Self::Group(group) => {
                group.create_column_ref(context, with_clause, alias, column, nullability)
            }
            Self::JoinExpr(join) => {
                join.create_column_ref(context, with_clause, column, nullability)
            }
            Self::MergeRecord(record) => {
                record.create_column_ref(context, with_clause, alias, column, nullability)
            }
            Self::CallTableFunc(ds) => ds.bind_column(context, alias, column, nullability),
            Self::DeferredTableFunc(ds) => ds.bind_column(context, alias, column, nullability),
            Self::UndefinedTableFunc(_) => {
                ColumnReferenceExpr::unknown_function_source_ref(context, alias, column)
            }
            Self::RangeTableSample() => todo!(),
            Self::RowFunctions(source) => {
                source.bind_column(context, with_clause, alias, column, nullability)
            }
        }
    }

    fn unknown(name: &str, alias: &AliasSpec) -> Self {
        Self::Unknown(name.to_string(), alias.clone())
    }

    fn push_alias_names(&self, names: &mut Vec<AliasSpec>) {
        match self {
            Self::CteQuery(d) => d.push_alias_names(names),
            Self::Table(d) => d.push_alias_names(names),
            Self::View(d) => d.push_alias_names(names),
            Self::JoinExpr(d) => d.push_alias_names(names),
            Self::MergeRecord(d) => d.push_alias_names(names),
            Self::Group(d) => d.push_alias_names(names),
            Self::SubQuery(d) => d.push_alias_names(names),
            Self::CallTableFunc(d) => d.push_alias_names(names),
            Self::DeferredTableFunc(d) => d.push_alias_names(names),
            Self::UndefinedTableFunc(d) => d.push_alias_names(names),
            Self::RowFunctions(functions) => names.extend(functions.get_aliases()),
            Self::RangeTableSample() => todo!(),
            Self::Unknown(_, alias) => names.push(alias.clone()),
        }
    }

    /// get alias name for data sources
    #[must_use]
    pub fn get_aliases(&self) -> Vec<AliasSpec> {
        let mut results = Vec::new();
        self.push_alias_names(&mut results);
        results
    }

    /// Resolve a unique visible alias. Missing or ambiguous aliases return `None`.
    #[must_use]
    pub fn resolve_alias(&self, name: &AliasName) -> Option<(&Self, Nullable)> {
        self.namespace().resolve_name(name).ok().flatten()?;
        match self {
            Self::CteQuery(d) => d.resolve_alias(self, name),
            Self::Table(d) => d.resolve_alias(self, name),
            Self::View(d) => d.resolve_alias(self, name),
            Self::JoinExpr(d) => d.resolve_alias(self, name),
            Self::MergeRecord(d) => d.resolve_alias(self, name),
            Self::Group(d) => d.resolve_alias(self, name),
            Self::SubQuery(d) => d.resolve_alias(self, name),
            Self::CallTableFunc(d) => d.resolve_alias(self, name),
            Self::DeferredTableFunc(d) => d.resolve_alias(self, name),
            Self::UndefinedTableFunc(d) => d.resolve_alias(self, name),
            Self::RangeTableSample() => todo!(),
            Self::Unknown(_, alias) => {
                if alias.name() == name {
                    Some((self, false))
                } else {
                    None
                }
            }
            Self::RowFunctions(rf) => rf.resolve_alias(self, name),
        }
    }

    /// Look up aliases that can conclusively supply a column.
    ///
    /// An unresolved source makes this lookup `Unknown`, even when another
    /// source has a known candidate. Only `NotFound` proves local absence.
    pub fn get_aliases_for_column<TParseContext>(
        &self,
        column: &ColumnName,
        context: &TParseContext,
        with_clause: &WithClause,
    ) -> ::sql_fun_core::LookupResult<Vec<AliasSpec>>
    where
        TParseContext: ParseContext,
    {
        self.aliases_for_column_resolved(column, context, with_clause)
            .into_lookup()
    }

    pub(crate) fn aliases_for_column_resolved<TContext: crate::sem::ParseContext>(
        &self,
        column: &ColumnName,
        context: &TContext,
        with_clause: &WithClause,
    ) -> ColumnCandidates {
        let mut result = Vec::new();
        match self {
            Self::CteQuery(d) => d.get_aliases_for_column(column, with_clause, &mut result),
            Self::Table(d) => d.get_aliases_for_column(column, context, &mut result),
            Self::View(d) => d.get_aliases_for_column(column, context, &mut result),
            Self::JoinExpr(d) => return d.column_candidates(column, context, with_clause),
            Self::MergeRecord(d) => return d.column_candidates(column, context, with_clause),
            Self::Group(d) => return d.column_candidates(column, context, with_clause),
            Self::SubQuery(d) => {
                d.get_aliases_for_column(column, context, with_clause, &mut result);
            }
            Self::CallTableFunc(d) => {
                d.get_aliases_for_column(column, context, with_clause, &mut result);
            }
            Self::UndefinedTableFunc(_) => return ColumnCandidates::unknown(),
            Self::DeferredTableFunc(d) => return d.candidates(column),
            Self::RangeTableSample() => todo!(),
            Self::Unknown(_, _) => return ColumnCandidates::unknown(),
            Self::RowFunctions(source) => {
                return source.candidates(column);
            }
        }
        ColumnCandidates::from(result)
    }
}

impl DataSource {
    pub(crate) fn with_group_alias(&self, alias: &AliasSpec) -> Self {
        Self::Group(Box::new(GroupedDataSource::new(self, alias)))
    }

    pub(crate) fn is_joined(&self) -> bool {
        match self {
            Self::JoinExpr(_) => true,
            Self::Group(group) => group.is_joined(),
            _ => false,
        }
    }

    fn group_column_source<'a, C: crate::sem::ParseContext>(
        &'a self,
        column: &ColumnName,
        context: &C,
        with: &WithClause,
    ) -> Option<(&'a Self, Nullable)> {
        // An unresolved descendant can compete even with a known column.
        if self
            .aliases_for_column_resolved(column, context, with)
            .is_unknown()
        {
            return None;
        }
        match self {
            Self::JoinExpr(join) => join.group_column_source(column, context, with),
            Self::Group(group) => group.column_source(column, context, with),
            _ => {
                if self
                    .aliases_for_column_resolved(column, context, with)
                    .is_empty()
                {
                    None
                } else {
                    Some((self, false))
                }
            }
        }
    }
}

impl DataSource {
    pub(crate) fn walk<V: crate::sem::traversal::SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), crate::sem::AnalysisError> {
        match self {
            Self::CteQuery(v) => v.walk(visitor)?,
            Self::Table(v) => v.walk(visitor)?,
            Self::View(v) => v.walk(visitor)?,
            Self::JoinExpr(v) => v.walk(visitor)?,
            Self::MergeRecord(v) => v.walk(visitor)?,
            Self::Group(v) => v.walk(visitor)?,
            Self::SubQuery(v) => v.walk(visitor)?,
            Self::CallTableFunc(value) => value.walk(visitor)?,
            Self::DeferredTableFunc(value) => value.walk(visitor)?,
            Self::UndefinedTableFunc(value) => value.walk(visitor)?,
            Self::RowFunctions(value) => value.walk(visitor)?,
            Self::RangeTableSample() | Self::Unknown(_, _) => {}
        }
        Ok(())
    }
}

impl DataSource {
    pub(crate) fn merge_action_available(&self) -> bool {
        match self {
            Self::MergeRecord(record) => record.merge_action_available(),
            _ => false,
        }
    }

    pub(crate) fn merge_row_target(
        &self,
        alias: &AliasName,
    ) -> Option<(&AliasName, MergeRecordRole, bool)> {
        match self {
            Self::MergeRecord(record) => record.merge_row_target(alias),
            _ => None,
        }
    }
}

impl DataSource {
    pub(crate) fn apply_function_output(
        &mut self,
        context: &mut impl ParseContext,
        output: &TableFunctionOutput,
    ) -> Result<(), AnalysisError> {
        match self {
            Self::CallTableFunc(source) => source.apply_output(context, output),
            Self::DeferredTableFunc(source) => source.apply_output(context, output),
            Self::UndefinedTableFunc(source) => {
                source.retain_output(output);
                Ok(())
            }
            _ => Ok(()),
        }
    }
}

impl DataSource {
    pub(crate) fn validate_function_aliases(
        &mut self,
        context: &mut impl ParseContext,
    ) -> Result<(), AnalysisError> {
        match self {
            Self::RowFunctions(source) => source.validate_aliases(context),
            Self::CallTableFunc(source) => source.validate_aliases(context),
            Self::DeferredTableFunc(source) => source.validate_aliases(context),
            _ => Ok(()),
        }
    }
}

impl DataSource {
    pub(crate) fn append_function_ordinality(&mut self) {
        match self {
            Self::CallTableFunc(source) => source.append_ordinality(),
            Self::DeferredTableFunc(source) => source.append_ordinality(),
            Self::RowFunctions(source) => source.append_ordinality(),
            _ => {}
        }
    }
}

impl DataSource {
    pub(crate) fn retain_rows_definition(&mut self, definition: &TableFunctionOutput) {
        if let Self::RowFunctions(source) = self {
            source.retain_outer_definition(definition);
        }
    }
}

impl DataSource {
    pub(super) fn known_function_columns(&self) -> Option<Vec<ColumnName>> {
        match self {
            Self::CallTableFunc(source) => Some(source.column_names()),
            Self::DeferredTableFunc(source) => source.known_column_names(),
            _ => None,
        }
    }
}
