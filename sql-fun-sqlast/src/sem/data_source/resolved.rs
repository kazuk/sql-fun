use crate::{
    context::ParseContext,
    sem::{
        AnalysisError, AnalysisProblem, ColumnName, CreateTrigger, DataSourceRowsFromFunc,
        FullName, SelectStatement, SemScalarExpr, TableName, ViewName, WithClause,
        data_source::{
            AliasSpec, CteDataSource, DataSource, JoinExprDataSource, JoinKind, SubQueryDataSource,
            TableDataSource, UndefinedTableFuncDataSource, ViewDataSource,
            join_expr::JoinUsingClause,
        },
        problem::DeleteProblemKind,
        type_system::ArgumentBindingCollection,
    },
};

impl DataSource {
    pub(crate) fn canonical_relation_column(
        &self,
        context: &impl ParseContext,
        column: &ColumnName,
    ) -> Option<ColumnName> {
        let target = match self {
            Self::Table(table) => table.name().full_name(),
            Self::View(view) => view.view_name().full_name(),
            _ => return None,
        };
        CreateTrigger::column(context, target, column)?
            .name()
            .clone()
    }

    pub(crate) fn resolved_rows(functions: &[DataSourceRowsFromFunc], alias: &AliasSpec) -> Self {
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
                    DeleteProblemKind::unreadable_cte(cte_name),
                ))?;
            } else {
                cte.require_readable()?;
            }
            return Ok(CteDataSource::new_data_source(cte_name, alias));
        }
        let canonical;
        let name = if context.dialect() == crate::ast::Dialect::sqlite() {
            canonical =
                crate::sem::search_path::expand_relation_search_path(context, name, |key| {
                    context.sqlite_relation_identity(key)
                });
            canonical.as_ref().unwrap_or(name)
        } else {
            name
        };
        if let Some(name) = context.resolve_relation_name(name) {
            Ok(Self::from_relation_name(context, &name, alias))
        } else {
            context.report_analysis_problem(AnalysisProblem::relation_not_found(
                &TableName::from(name.clone()),
            ))?;
            Ok(Self::unknown(&name.to_string(), alias))
        }
    }
    fn from_relation_name(context: &impl ParseContext, name: &FullName, alias: &AliasSpec) -> Self {
        if context.get_table(&TableName::from(name.clone())).is_some() {
            TableDataSource::new_data_source(&TableName::from(name.clone()), alias)
        } else {
            Self::View(ViewDataSource::from_name(
                &ViewName::from_full_name(name),
                alias,
            ))
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
        let mut source = Self::table_function(&overload, arguments.to_vec().into(), alias);
        source.record_function_name(name, &context.resolve_function_name_with_search_path(name));
        Ok(source)
    }
}
