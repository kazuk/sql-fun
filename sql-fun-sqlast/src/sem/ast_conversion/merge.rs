use crate::names::AliasName;
use crate::names::FullName;
use crate::sem::ast_conversion::insert::InsertTargetBinding;
use crate::{
    CacheContext,
    ast::{
        Expr, Target,
        document::{CmdType, Merge, MergeMatchKind, MergeWhen, OverridingKind},
    },
    context::ParseContext,
    names::TableName,
    sem::{
        AliasSpec, AnalysisError, AnalysisProblem, DataSource, FromClause, InsertClauseSemantics,
        MergeAction, MergeDestination, MergeMatchKind as SemMergeMatchKind, MergeStatement,
        MergeWhen as SemMergeWhen, MergeWhenCollection, PgBuiltInType, ResolutionFailure,
        ResultColumnCollection, ScalarAnalysisContext, SemScalarExpr, UpdateAssignmentCollection,
        WithClause, parameter_resolution::ParameterResolution,
    },
};

impl MergeStatement {
    pub(crate) fn from_ast(
        ast: &Merge,
        context: &mut impl ParseContext,
        cache: &mut CacheContext,
    ) -> Result<Self, AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = Self::from_ast_in(ast, context, None).and_then(|mut merge| {
            if context.parameter_inference_needed() {
                let constraints = ParameterResolution::infer_merge(&mut merge, context, cache)?;
                context.set_parameter_constraints(&constraints);
            }
            Ok(merge)
        });
        context.finish_analysis(result)
    }
}

impl MergeStatement {
    pub(crate) fn ast_require_version(
        context: &impl ParseContext,
        minimum: u32,
    ) -> Result<(), AnalysisError> {
        Self::require_version(context, minimum)
    }
}

impl MergeStatement {
    pub(crate) fn from_ast_in(
        ast: &Merge,
        context: &mut impl ParseContext,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<Self, AnalysisError> {
        Self::ast_require_version(context, 15)?;
        if ast.with_clause.recursive {
            return Err(AnalysisError::unexpected_input_error(
                "WITH RECURSIVE is not supported for MERGE",
            ));
        }
        if !ast.returning_list.is_empty() {
            Self::ast_require_version(context, 17)?;
        }
        let requested = TableName::from(FullName::from_ast_name(&ast.relation.name)?);
        // MERGE eligibility is separate from the ordinary INSERT/UPDATE/DELETE
        // capabilities of a view. Never infer it from those flags.
        let Some(table) = MergeDestination::resolve(context, &requested)? else {
            context.report_analysis_problem(AnalysisProblem::relation_not_found(&requested))?;
            return Err(AnalysisError::unexpected_input_error(
                "MERGE target relation not found",
            ));
        };
        if table.is_materialized_view() {
            return Err(AnalysisError::unexpected_input_error(
                "MERGE cannot target a materialized view",
            ));
        }
        if table.view_support().is_some() {
            Self::ast_require_version(context, 17)?;
        }
        let alias = ast
            .relation
            .alias
            .as_ref()
            .map(AliasSpec::from_ast)
            .unwrap_or_else(|| AliasSpec::from_local_name(&requested.full_name().local_name()));
        let with = WithClause::from_ast(&ast.with_clause, context, outer)?;
        let target = DataSource::resolve_relation(
            context,
            table.name().full_name(),
            &alias,
            &WithClause::default(),
        )?;
        let source = DataSource::from_ast(
            &ast.source_relation,
            context,
            &with,
            &FromClause::default(),
            None,
        )?;
        if source.namespace().contains_alias(alias.name()) {
            context.report_analysis_problem(AnalysisProblem::duplicate_alias(alias.name()))?;
        }
        let on_scope = ScalarAnalysisContext::for_merge_on(&with, &target, &source);
        let on = SemScalarExpr::from_ast_merge_predicate(&ast.join_condition, context, &on_scope)?;
        let clauses =
            Self::from_ast_clauses(ast, context, &with, &target, &source, &table, &alias)?;
        if let Some(support) = table.view_support()
            && !support.supports_actions(
                clauses.permits_insert(),
                clauses.permits_update(),
                clauses.permits_delete(),
            )
        {
            return Err(AnalysisError::unexpected_input_error(
                "view does not support all MERGE action types",
            ));
        }
        let returning_scope = ScalarAnalysisContext::for_merge_returning(
            context,
            &with,
            &target,
            &source,
            clauses.source_nullable(),
            clauses.permits_insert(),
            clauses.permits_delete(),
        );
        let returning = ResultColumnCollection::resolve_for_merge(
            &ast.returning_list,
            context,
            &returning_scope,
        )?;
        let mut statement = Self::new(
            table.name(),
            &WithClause::ast_local_definitions(&ast.with_clause, with.clone()),
            on_scope.sources(),
            &on,
            &clauses,
            &returning,
            ast.relation.inherit,
        );
        statement.set_view_support(table.view_support());
        statement.validate_assignments(context)?;
        Ok(statement)
    }
}

impl MergeStatement {
    pub(crate) fn from_ast_clauses(
        ast: &Merge,
        context: &mut impl ParseContext,
        with: &WithClause,
        target: &DataSource,
        source: &DataSource,
        table: &MergeDestination,
        alias: &AliasSpec,
    ) -> Result<MergeWhenCollection, AnalysisError> {
        let target_scope = ScalarAnalysisContext::for_merge_on(with, target, source);
        let mut clauses = MergeWhenCollection::default();
        let mut terminal = [false; 3];
        for clause in &ast.merge_when_clauses {
            let index = SemMergeMatchKind::ast_index(&clause.match_kind);
            if terminal[index] {
                return Err(AnalysisError::unexpected_input_error(
                    "unreachable MERGE WHEN clause after unconditional action",
                ));
            }
            terminal[index] = clause.condition.is_none();
            let kind = SemMergeMatchKind::from_ast_merge_match_kind(&clause.match_kind, context)?;
            let scope = ScalarAnalysisContext::for_merge_when(with, target, source, kind);
            let condition = clause
                .condition
                .as_ref()
                .map(|expr| SemScalarExpr::from_ast_merge_predicate(expr, context, &scope))
                .transpose()?;
            let action =
                MergeAction::from_ast_action(clause, context, &scope, &target_scope, table, alias)?;
            clauses.push(&SemMergeWhen::new(kind, &condition, &action));
        }
        Ok(clauses)
    }
}

#[cfg(test)]
mod tests;
impl SemMergeMatchKind {
    pub(crate) const fn ast_index(ast: &MergeMatchKind) -> usize {
        match ast {
            MergeMatchKind::Matched => 0,
            MergeMatchKind::NotMatchedBySource => 1,
            MergeMatchKind::NotMatchedByTarget => 2,
        }
    }
}

impl SemMergeMatchKind {
    pub(crate) fn from_ast_merge_match_kind(
        ast: &MergeMatchKind,
        context: &impl ParseContext,
    ) -> Result<Self, AnalysisError> {
        match ast {
            MergeMatchKind::Matched => Ok(Self::matched()),
            MergeMatchKind::NotMatchedByTarget => Ok(Self::by_target()),
            MergeMatchKind::NotMatchedBySource => Self::from_ast_by_source(context),
        }
    }
}

impl SemMergeMatchKind {
    pub(crate) const fn ast_permits(ast: &MergeMatchKind, command: CmdType) -> bool {
        match command {
            CmdType::Nothing => true,
            CmdType::Insert => matches!(ast, MergeMatchKind::NotMatchedByTarget),
            CmdType::Update | CmdType::Delete => !matches!(ast, MergeMatchKind::NotMatchedByTarget),
            _ => false,
        }
    }
}

impl SemMergeMatchKind {
    fn from_ast_by_source(context: &impl ParseContext) -> Result<Self, AnalysisError> {
        MergeStatement::ast_require_version(context, 17)?;
        Ok(Self::by_source())
    }
}

impl MergeAction {
    pub(crate) fn from_ast_action(
        ast: &MergeWhen,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        target_scope: &ScalarAnalysisContext<'_>,
        target: &MergeDestination,
        alias: &AliasSpec,
    ) -> Result<Self, AnalysisError> {
        if !SemMergeMatchKind::ast_permits(&ast.match_kind, ast.command_type) {
            return Err(AnalysisError::unexpected_input_error(
                "illegal MERGE action for WHEN match kind",
            ));
        }
        match ast.command_type {
            CmdType::Update => {
                Self::from_ast_update_action(ast, context, scope, target_scope, target, alias)
            }
            CmdType::Insert => {
                Self::from_ast_insert_action(ast, context, scope, target_scope, target, alias)
            }
            CmdType::Delete => Ok(Self::delete()),
            CmdType::Nothing => Ok(Self::nothing()),
            _ => Err(AnalysisError::unexpected_input_error(
                "unsupported MERGE action",
            )),
        }
    }
}

impl MergeAction {
    pub(crate) fn from_ast_update_action(
        ast: &MergeWhen,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        target_scope: &ScalarAnalysisContext<'_>,
        target: &MergeDestination,
        alias: &AliasSpec,
    ) -> Result<Self, AnalysisError> {
        if !target.update().is_writable() {
            return Err(AnalysisError::unexpected_input_error(
                "MERGE target does not accept UPDATE assignments",
            ));
        }
        let result = UpdateAssignmentCollection::resolve(
            context,
            &ast.target_list,
            target.update(),
            alias,
            target_scope,
            scope,
        );
        let Some(assignments) = ResolutionFailure::finish(result, context)? else {
            return Ok(Self::unresolved());
        };
        Ok(Self::update(&assignments))
    }
}

impl MergeAction {
    pub(crate) fn from_ast_insert_action(
        ast: &MergeWhen,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        target_scope: &ScalarAnalysisContext<'_>,
        target: &MergeDestination,
        alias: &AliasSpec,
    ) -> Result<Self, AnalysisError> {
        if !target.insert().is_writable() {
            return Err(AnalysisError::unexpected_input_error(
                "MERGE target does not accept INSERT assignments",
            ));
        }
        let values = if ast.values.is_empty() {
            None
        } else {
            Some(ast.values.as_slice())
        };
        let result = InsertClauseSemantics::resolve(
            context,
            &ast.target_list,
            &InsertTargetBinding::new(target.insert(), alias, target_scope),
            values,
            scope,
            Some(&ast.overriding),
        );
        let Some(clause) = ResolutionFailure::finish(result, context)? else {
            return Ok(Self::unresolved());
        };
        let overriding = InsertClauseSemantics::ast_merge_override(&ast.overriding);
        Ok(Self::insert(&clause, overriding.as_deref()))
    }
}

impl InsertClauseSemantics {
    pub(crate) fn ast_merge_override(ast: &OverridingKind) -> Option<String> {
        match ast {
            OverridingKind::NotSet => None,
            OverridingKind::UserValue => Some("user".to_owned()),
            OverridingKind::SystemValue => Some("system".to_owned()),
        }
    }
}

impl SemScalarExpr {
    pub(crate) fn from_ast_merge_predicate(
        ast: &Expr,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        let value = Self::from_ast_in(ast, context, scope)?;
        if let Some(ty) = value.get_type()
            && ty != PgBuiltInType::bool()
            && !value
                .as_const()
                .is_some_and(|v| v.is_null() || v.as_string().is_some())
            && context
                .get_implicit_cast(&ty, &PgBuiltInType::bool())
                .is_none()
        {
            return Err(AnalysisError::unexpected_input_error(
                "MERGE condition must have boolean type",
            ));
        }
        Ok(value)
    }
}

impl ResultColumnCollection {
    fn resolve_for_merge(
        targets: &[Target],
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        let mut results = Self::default();
        for target in targets {
            if let Some(name) = target.source_star_name() {
                let qualifier = AliasName::from_ast_star_qualifier(name)?;
                if scope.check_returning_star(context, qualifier.as_ref())? {
                    continue;
                }
                let projection_qualifier = qualifier.as_ref().map(|alias| {
                    scope
                        .merge_row_target(alias)
                        .map_or(alias, |(target, _, _)| target)
                });
                if !results.expand_projection(
                    context,
                    scope.sources(),
                    scope,
                    target.projection_location(),
                    projection_qualifier,
                    qualifier
                        .as_ref()
                        .filter(|alias| scope.merge_row_target(alias).is_some()),
                )? {
                    return Err(AnalysisError::unexpected_input_error(
                        "MERGE RETURNING star source is not visible",
                    ));
                }
            } else {
                results.append_resolved_target(target, context, scope)?;
            }
        }
        Ok(results)
    }
}
