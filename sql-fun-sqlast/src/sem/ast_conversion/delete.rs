use crate::sem::SemScalarExpr;
use crate::{
    CacheContext,
    ast::{CurrentOf, Delete, Dialect, Expr},
    context::ParseContext,
    names::{FullName, TableName},
    sem::{
        AliasSpec, AnalysisError, AnalysisProblem, DataSource, DeleteCursor, DeleteDestination,
        DeletePredicate, DeleteStatement, FromClause, PgBuiltInType, ResultColumnCollection,
        ScalarAnalysisContext, WithClause, parameter_resolution::ParameterResolution,
        problem::DeleteProblemKind,
    },
};

impl DeleteStatement {
    pub(crate) fn from_ast(
        ast: &Delete,
        context: &mut impl ParseContext,
        cache: &mut CacheContext,
    ) -> Result<Self, AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = Self::from_ast_in(ast, context, None).and_then(|mut delete| {
            if context.parameter_inference_needed() {
                let constraints = ParameterResolution::infer_delete(&mut delete, context, cache)?;
                context.set_parameter_constraints(&constraints);
            }
            Ok(delete)
        });
        context.finish_analysis(result)
    }
}

impl DeleteStatement {
    pub(crate) fn from_ast_in(
        ast: &Delete,
        context: &mut impl ParseContext,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<Self, AnalysisError> {
        let trigger_outer = if let Some(scope) = outer {
            scope.trigger_scope()
        } else {
            None
        };
        let name = FullName::from_ast_name(&ast.relation.name)?;
        let name = if let Some(scope) = outer {
            scope.trigger_relation_name(context, &name)?
        } else {
            name
        };
        let requested = TableName::from(name);
        let table = Self::from_ast_destination(
            ast,
            context,
            &requested,
            outer.is_some_and(ScalarAnalysisContext::in_rule_definition),
        )?;
        Self::validate_ast_index_hint(ast, context, &table)?;
        let with = WithClause::from_ast(&ast.with, context, outer)?;
        let alias = ast
            .relation
            .alias
            .as_ref()
            .map(AliasSpec::from_ast)
            .unwrap_or_else(|| AliasSpec::from_local_name(&requested.full_name().local_name()));
        let from = Self::from_ast_sources(ast, context, &table, &alias, &with, trigger_outer)?;
        if let Some(outer) = outer {
            outer.validate_rule_sources(context, &from)?;
        }
        let scope = ScalarAnalysisContext::nested(&with, &from, trigger_outer);
        let predicate = Self::from_ast_predicate(ast, context, table.name(), &scope)?;
        let returning =
            Self::from_ast_returning_scope(ast, context, &requested, &table, &alias, &scope)?;
        let mut delete = Self::new(
            table.name(),
            table.found(),
            &WithClause::ast_local_definitions(&ast.with, with),
            &from,
            &predicate,
            &returning,
        );
        delete.set_modifiers(
            table.view_support(),
            &ast.relation.indexed,
            ast.relation.not_indexed,
            ast.relation.inherit,
        );
        Ok(delete)
    }
}

impl DeleteStatement {
    pub(crate) fn from_ast_destination(
        ast: &Delete,
        context: &mut impl ParseContext,
        requested: &TableName,
        rule_definition: bool,
    ) -> Result<DeleteDestination, AnalysisError> {
        let destination = if rule_definition {
            DeleteDestination::resolve_rule_target(context, requested)
        } else {
            DeleteDestination::resolve(context, requested)
        };
        let Some(table) = destination else {
            context.report_analysis_problem(AnalysisProblem::relation_not_found(requested))?;
            return Ok(DeleteDestination::missing(requested));
        };
        if !table.is_deletable() {
            context.report_analysis_problem(AnalysisProblem::delete(
                DeleteProblemKind::read_only_target(),
            ))?;
        }
        if !ast.returning.is_empty()
            && table
                .view_support()
                .is_some_and(|s| !s.supports_returning())
        {
            context.report_analysis_problem(AnalysisProblem::delete(
                DeleteProblemKind::view_operation(),
            ))?;
        }
        Ok(table)
    }
}

impl DeleteStatement {
    pub(crate) fn validate_ast_index_hint(
        ast: &Delete,
        context: &mut impl ParseContext,
        table: &DeleteDestination,
    ) -> Result<(), AnalysisError> {
        if let Some(index) = &ast.relation.indexed {
            let name = FullName::with_schema(
                table
                    .name()
                    .full_name()
                    .schema()
                    .map(|schema| schema.as_str())
                    .unwrap_or("main"),
                index,
            );
            if context
                .get_index(&name)
                .is_none_or(|index| !index.belongs_to(table.name()))
            {
                context.report_analysis_problem(AnalysisProblem::delete(
                    DeleteProblemKind::index_hint(),
                ))?;
            }
        }
        Ok(())
    }
}

impl DeleteStatement {
    pub(crate) fn from_ast_sources(
        ast: &Delete,
        context: &mut impl ParseContext,
        table: &DeleteDestination,
        alias: &AliasSpec,
        with: &WithClause,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<FromClause, AnalysisError> {
        // USING subqueries cannot implicitly see the DELETE target. WHERE and
        // RETURNING subqueries use the complete scope assembled below.
        let mut from = DataSource::from_ast_list_in(&ast.using, context, with, outer)?;
        let source = DataSource::resolve_relation(
            context,
            table.name().full_name(),
            alias,
            &WithClause::default(),
        )?;
        from.validate_source(context, &source)?;
        from.push(source);
        Ok(from)
    }
}

impl DeleteStatement {
    pub(crate) fn from_ast_predicate(
        ast: &Delete,
        context: &mut impl ParseContext,
        target: &TableName,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Option<DeletePredicate>, AnalysisError> {
        if let Some(expression) = &ast.predicate
            && let Some(cursor) =
                DeleteCursor::from_ast_delete_current_of(expression, context, target)?
        {
            return Ok(Some(DeletePredicate::current_of(&cursor)));
        }
        let predicate = SemScalarExpr::from_ast_optional(&ast.predicate, context, scope)?;
        if context.dialect() == Dialect::postgres()
            && let Some(value) = &predicate
            && let Some(ty) = value.get_type()
            && ty != PgBuiltInType::bool()
            && !value
                .as_const()
                .is_some_and(|c| c.is_null() || c.as_string().is_some())
            && context
                .get_implicit_cast(&ty, &PgBuiltInType::bool())
                .is_none()
        {
            context
                .report_analysis_problem(AnalysisProblem::delete(DeleteProblemKind::predicate()))?;
        }
        Ok(predicate.as_ref().map(DeletePredicate::expression))
    }
}

impl DeleteStatement {
    pub(crate) fn from_ast_returning_scope(
        ast: &Delete,
        context: &mut impl ParseContext,
        requested: &TableName,
        table: &DeleteDestination,
        alias: &AliasSpec,
        outer: &ScalarAnalysisContext<'_>,
    ) -> Result<ResultColumnCollection, AnalysisError> {
        let with = outer.with_clause();
        let from = outer.sources();
        if context.dialect() == Dialect::sqlite() {
            let returning_alias = AliasSpec::from_local_name(&requested.full_name().local_name());
            let mut returning_from = FromClause::default();
            returning_from.push(DataSource::resolve_relation(
                context,
                table.name().full_name(),
                &returning_alias,
                &WithClause::default(),
            )?);
            let mut scope = ScalarAnalysisContext::nested(with, &returning_from, None);
            scope.set_delete_returning_target(returning_alias.name(), context);
            ResultColumnCollection::resolve_for_delete(
                &ast.returning,
                context,
                &returning_from,
                &scope,
            )
        } else {
            let mut scope = ScalarAnalysisContext::nested(with, from, Some(outer));
            scope.set_delete_returning_target(alias.name(), context);
            ResultColumnCollection::resolve_for_delete(&ast.returning, context, from, &scope)
        }
    }
}

#[cfg(test)]
mod tests;
impl DeleteCursor {
    pub(crate) fn from_ast_delete_current_of(
        ast: &Expr,
        context: &mut impl ParseContext,
        target: &TableName,
    ) -> Result<Option<Self>, AnalysisError> {
        match ast {
            Expr::CurrentOf(cursor) => Self::from_ast(cursor, context, target).map(Some),
            _ => Ok(None),
        }
    }
}

impl DeleteCursor {
    pub(crate) fn from_ast(
        ast: &CurrentOf,
        context: &mut impl ParseContext,
        target: &TableName,
    ) -> Result<Self, AnalysisError> {
        if ast.parameter != 0 {
            context.report_analysis_problem(AnalysisProblem::delete(
                DeleteProblemKind::cursor_parameter(ast.parameter),
            ))?;
            return Ok(Self::parameter(ast.parameter));
        }
        let cursor = Self::new(&ast.name, context.resolve_cursor(&ast.name));
        let problem = match cursor.binding() {
            None => Some(DeleteProblemKind::missing_cursor(&ast.name)),
            Some(info) if info.updatable() == Some(false) => {
                Some(DeleteProblemKind::non_updatable_cursor(&ast.name))
            }
            Some(info) => info
                .target()
                .filter(|relation| *relation != target)
                .map(|actual| DeleteProblemKind::cursor_target_mismatch(&ast.name, target, actual)),
        };
        if let Some(kind) = problem {
            context.report_analysis_problem(AnalysisProblem::delete(kind))?;
        }
        Ok(cursor)
    }
}
