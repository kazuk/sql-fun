use crate::sem::data_source::{
    AliasSpec, CteDataSource, DataSource, JoinExprDataSource, JoinKind, SubQueryDataSource,
    TableDataSource, UndefinedTableFuncDataSource, ViewDataSource, join_expr::JoinUsingClause,
};
use crate::sem::{
    AnalysisError, AnalysisProblem, FullName, ParseContext, SelectStatement, SemScalarExpr,
    TableName, ViewName, WithClause, type_system::ArgumentBindingCollection,
};

impl DataSource {
    pub(crate) fn trigger_column(
        &self,
        context: &impl ParseContext,
        column: &crate::sem::ColumnName,
    ) -> Option<crate::sem::ColumnName> {
        let target = match self {
            Self::Table(table) => table.name().full_name(),
            Self::View(view) => view.view_name().full_name(),
            _ => return None,
        };
        crate::sem::CreateTrigger::column(context, target, column)?
            .name()
            .clone()
    }

    pub(crate) fn resolved_rows(
        functions: &[crate::sem::DataSourceRowsFromFunc],
        alias: &AliasSpec,
    ) -> Self {
        Self::RowFunctions(super::DataSourceRowFunctions::new(
            &functions.to_vec().into(),
            alias,
        ))
    }
    pub(crate) fn resolve_relation<C: ParseContext>(
        context: &mut C,
        name: &FullName,
        alias: &AliasSpec,
        with: &WithClause,
    ) -> Result<Self, AnalysisError> {
        if let Some(cte_name) = with.resolve_name(context, name) {
            let cte = with.get(cte_name).expect("resolved CTE name is stored");
            if cte.is_unreadable_delete() {
                context.report_analysis_problem(AnalysisProblem::delete(
                    crate::sem::problem::DeleteProblemKind::unreadable_cte(cte_name),
                ))?;
            } else {
                cte.require_readable()?;
            }
            return Ok(CteDataSource::new_data_source(cte_name, alias));
        }
        let canonical;
        let name = if context.dialect() == crate::ast::Dialect::sqlite() {
            canonical = crate::sem::search_path::expand_search_path_inst(context, name, |key| {
                context.sqlite_relation_identity(key)
            });
            canonical.as_ref().unwrap_or(name)
        } else {
            name
        };
        if context.get_table(&TableName::from(name.clone())).is_some() {
            Ok(TableDataSource::new_data_source(
                &TableName::from(name.clone()),
                alias,
            ))
        } else if context.get_view(&ViewName::from_full_name(name)).is_some() {
            Ok(Self::View(ViewDataSource::from_name(
                &ViewName::from_full_name(name),
                alias,
            )))
        } else {
            context.report_analysis_problem(AnalysisProblem::relation_not_found(
                &TableName::from(name.clone()),
            ))?;
            Ok(Self::unknown(&name.to_string(), alias))
        }
    }

    pub(crate) fn resolved_query(query: &SelectStatement, alias: &AliasSpec) -> Self {
        SubQueryDataSource::new_data_source(query, alias)
    }

    pub(crate) fn resolved_join(
        left: &Self,
        right: &Self,
        kind: &JoinKind,
        condition: &Option<SemScalarExpr>,
        alias: &Option<AliasSpec>,
        using: &Option<JoinUsingClause>,
        natural: bool,
    ) -> Self {
        JoinExprDataSource::new_data_source(kind, left, right, condition, alias, using, natural)
    }

    pub(crate) fn resolve_function<C: ParseContext>(
        context: &mut C,
        name: &FullName,
        arguments: &[SemScalarExpr],
        alias: &AliasSpec,
    ) -> Result<Self, AnalysisError> {
        let arg_types = ArgumentBindingCollection::from_expr_list(arguments);
        let Some(overloads) = context.get_function_by_name(name) else {
            context.report_analysis_problem(AnalysisProblem::table_function_not_found(name))?;
            return Ok(Self::UndefinedTableFunc(
                UndefinedTableFuncDataSource::from_name(name, arguments, alias),
            ));
        };
        let Some(overload) = overloads.resolve_from_catalog(context, &arg_types) else {
            context.report_analysis_problem(
                AnalysisProblem::parameter_type_mismatch_for_table_function(name, &arg_types),
            )?;
            return Ok(Self::UndefinedTableFunc(
                UndefinedTableFuncDataSource::from_name(name, arguments, alias),
            ));
        };
        Ok(Self::table_function(
            &overload,
            arguments.to_vec().into(),
            alias,
        ))
    }
}
