mod alias;
mod analyze;
mod cte;
mod group;
mod join_expr;
mod join_kind;
mod projection;
mod resolved;
mod rows_fun;
mod sub_query;
mod table;
mod table_fun;
mod undefined;
mod view;

pub(crate) use self::join_expr::JoinUsingClause;
pub use self::{
    alias::{AliasName, AliasSpec},
    cte::CteDataSource,
    group::GroupedDataSource,
    join_expr::JoinExprDataSource,
    join_kind::JoinKind,
    rows_fun::{DataSourceRowFunctions, DataSourceRowsFromFunc},
    sub_query::SubQueryDataSource,
    table::TableDataSource,
    table_fun::TableFunctionDataSource,
    undefined::UndefinedTableFuncDataSource,
    view::ViewDataSource,
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
    /// SQLite parenthesized source with a group alias.
    Group(Box<GroupedDataSource>),
    /// sub-query
    SubQuery(Box<SubQueryDataSource>),
    /// table function
    CallTableFunc(TableFunctionDataSource),
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
            Self::View(viewds) => ColumnReferenceExpr::view_source_ref(
                context,
                alias,
                column,
                viewds.view_name(),
                nullability,
            ),
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
            // resolve_alias not returns this variants
            Self::JoinExpr(_) => unreachable!(),
            Self::CallTableFunc(ds) => ColumnReferenceExpr::function_source_ref(
                context,
                alias,
                column,
                ds.function(),
                ds.arguments(),
                nullability,
            ),
            Self::UndefinedTableFunc(_ds) => {
                ColumnReferenceExpr::unknown_function_source_ref(context, alias, column)
            }
            Self::RangeTableSample() => todo!(),
            Self::RowFunctions(_) => todo!(),
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
            Self::Group(d) => d.push_alias_names(names),
            Self::SubQuery(d) => d.push_alias_names(names),
            Self::CallTableFunc(d) => d.push_alias_names(names),
            Self::UndefinedTableFunc(d) => d.push_alias_names(names),
            Self::RowFunctions(_data_source_row_functions) => todo!(),
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

    /// resolve alias name to data source
    #[must_use]
    pub fn resolve_alias(&self, name: &AliasName) -> Option<(&Self, Nullable)> {
        match self {
            Self::CteQuery(d) => d.resolve_alias(self, name),
            Self::Table(d) => d.resolve_alias(self, name),
            Self::View(d) => d.resolve_alias(self, name),
            Self::JoinExpr(d) => d.resolve_alias(name),
            Self::Group(d) => d.resolve_alias(self, name),
            Self::SubQuery(d) => d.resolve_alias(self, name),
            Self::CallTableFunc(d) => d.resolve_alias(self, name),
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

    /// list alias name for column name
    pub fn get_aliases_for_column<TParseContext>(
        &self,
        column: &ColumnName,
        context: &TParseContext,
        with_clause: &WithClause,
    ) -> Vec<AliasSpec>
    where
        TParseContext: ParseContext,
    {
        self.aliases_for_column_resolved(column, context, with_clause)
    }

    pub(crate) fn aliases_for_column_resolved<TContext: crate::sem::ParseContext>(
        &self,
        column: &ColumnName,
        context: &TContext,
        with_clause: &WithClause,
    ) -> Vec<AliasSpec> {
        let mut result = Vec::new();
        match self {
            Self::CteQuery(d) => d.get_aliases_for_column(column, with_clause, &mut result),
            Self::Table(d) => d.get_aliases_for_column(column, context, &mut result),
            Self::View(d) => d.get_aliases_for_column(column, context, &mut result),
            Self::JoinExpr(d) => {
                d.get_aliases_for_column(column, context, with_clause, &mut result);
            }
            Self::Group(d) => d.get_aliases_for_column(column, context, with_clause, &mut result),
            Self::SubQuery(d) => {
                d.get_aliases_for_column(column, context, with_clause, &mut result);
            }
            Self::CallTableFunc(d) => {
                d.get_aliases_for_column(column, context, with_clause, &mut result);
            }
            Self::UndefinedTableFunc(_) => {}
            Self::RangeTableSample() => todo!(),
            Self::Unknown(_, _) => {}
            Self::RowFunctions(_) => todo!(),
        }
        result
    }
}

impl DataSource {
    pub(crate) fn with_group_alias(&self, alias: &AliasSpec) -> Self {
        Self::Group(Box::new(GroupedDataSource::new(self, alias)))
    }

    fn is_joined(&self) -> bool {
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
            Self::Group(v) => v.walk(visitor)?,
            Self::SubQuery(v) => v.walk(visitor)?,
            Self::CallTableFunc(value) => value.walk(visitor)?,
            Self::UndefinedTableFunc(value) => value.walk(visitor)?,
            Self::RowFunctions(value) => value.walk(visitor)?,
            Self::RangeTableSample() | Self::Unknown(_, _) => {}
        }
        Ok(())
    }
}
