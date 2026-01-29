mod alias;
mod analyze;
mod cte;
mod join_expr;
mod join_kind;
mod rows_fun;
mod sub_query;
mod table;
mod table_fun;
mod undefined;
mod view;

use sql_fun_core::IVec;

pub use self::{
    alias::{AliasName, AliasSpec},
    cte::CteDataSource,
    join_expr::JoinExprDataSource,
    join_kind::JoinKind,
    rows_fun::{DataSourceRowFunctions, DataSourceRowsFromFunc},
    sub_query::SubQueryDataSource,
    table::TableDataSource,
    table_fun::TableFunctionDataSource,
    undefined::UndefinedTableFuncDataSource,
    view::ViewDataSource,
};

use crate::{
    sem::{
        AnalysisError, ColumnName, FromClause, FullName, ParseContext, SemScalarExpr, WithClause,
        analyze_scaler_expr, scalar_expr::ColumnReferenceExpr,
        type_system::ArgumentBindingCollection,
    },
    syn::{ListOpt, ScanToken},
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
    pub(super) fn create_column_ref<TParseContext>(
        &self,
        mut context: TParseContext,
        with_clause: &WithClause,
        alias: &AliasName,
        column: &ColumnName,
        nullability: bool,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        match self {
            Self::CteQuery(cteds) => {
                let column_ref =
                    cteds.create_column_ref(&mut context, with_clause, column, nullability)?;
                Ok((SemScalarExpr::ColumnRef(column_ref), context))
            }
            Self::Table(tableds) => {
                let column_ref = tableds.create_column_ref(&mut context, column, nullability)?;
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
                let column_ref = subqds.create_column_ref(&mut context, column, nullability)?;
                Ok((SemScalarExpr::ColumnRef(column_ref), context))
            }
            Self::Unknown(_, _) => {
                // DataSource::Unknown ready report_problem in from_clause analysis
                ColumnReferenceExpr::unknown_source_ref(context, alias, column)
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

    #[track_caller]
    fn unknown(name: &str, alias: &AliasSpec) -> Self {
        #[cfg(test)]
        panic!("unknown datasource {name}, alias {alias:?}");

        #[cfg(not(test))]
        Self::Unknown(name.to_string(), alias.clone())
    }

    fn push_alias_names(&self, names: &mut Vec<AliasSpec>) {
        match self {
            Self::CteQuery(d) => d.push_alias_names(names),
            Self::Table(d) => d.push_alias_names(names),
            Self::View(d) => d.push_alias_names(names),
            Self::JoinExpr(d) => d.push_alias_names(names),
            Self::SubQuery(d) => d.push_alias_names(names),
            Self::CallTableFunc(d) => d.push_alias_names(names),
            Self::UndefinedTableFunc(d) => d.push_alias_names(names),
            Self::RowFunctions(_data_source_row_functions) => todo!(),
            Self::RangeTableSample() => todo!(),
            Self::Unknown(_, _) => todo!(),
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
    pub fn resolve_alias(&self, name: &AliasName) -> Option<(&DataSource, Nullable)> {
        match self {
            Self::CteQuery(d) => d.resolve_alias(self, name),
            Self::Table(d) => d.resolve_alias(self, name),
            Self::View(d) => d.resolve_alias(self, name),
            Self::JoinExpr(d) => d.resolve_alias(name),
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
        let mut result = Vec::new();
        match self {
            Self::CteQuery(d) => d.get_aliases_for_column(column, with_clause, &mut result),
            Self::Table(d) => d.get_aliases_for_column(column, context, &mut result),
            Self::View(d) => d.get_aliases_for_column(column, context, &mut result),
            Self::JoinExpr(d) => {
                d.get_aliases_for_column(column, context, with_clause, &mut result);
            }
            Self::SubQuery(d) => {
                d.get_aliases_for_column(column, context, with_clause, &mut result);
            }
            Self::CallTableFunc(d) => {
                d.get_aliases_for_column(column, context, with_clause, &mut result);
            }
            Self::UndefinedTableFunc(_) => {}
            Self::RangeTableSample() => todo!(),
            Self::Unknown(_, _) => todo!(),
            Self::RowFunctions(_) => todo!(),
        }
        result
    }

    fn call_table_func<TParseContext>(
        mut context: TParseContext,
        with_clause: &WithClause,
        from_clause: &FromClause,
        call: crate::syn::FuncCall,
        alias: Option<crate::syn::Alias>,
        tokens: &IVec<ScanToken>,
    ) -> Result<(DataSource, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let func_name = FullName::try_from(call.get_funcname())?;

        let alias_name = if let Some(alias) = alias {
            let (alias, new_context) = AliasSpec::analyze(context, &func_name, &alias)?;
            context = new_context;
            alias
        } else {
            AliasSpec::from_local_name(&func_name.local_name())
        };

        let mut arg_exprs = Vec::new();
        let mut arg_types = ArgumentBindingCollection::default();
        if let Some(args) = call.get_args().as_inner() {
            for arg in args {
                let (sarg, new_context) =
                    analyze_scaler_expr(context, with_clause, from_clause, arg, tokens)?;
                context = new_context;
                arg_types.push(sarg.get_type());
                arg_exprs.push(sarg);
            }
        }

        let ds = context.resolve_table_function(
            &call,
            func_name,
            &alias_name,
            arg_exprs.into(),
            arg_types,
        )?;

        Ok((ds, context))
    }
}
