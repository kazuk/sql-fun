use crate::sem::ast_conversion::insert::InsertTargetBinding;
use crate::sem::{AstConversion, convert};
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

impl AstConversion<'_, Merge> {
    pub(crate) fn resolve(
        &self,
        context: &mut impl ParseContext,
        cache: &mut CacheContext,
    ) -> Result<MergeStatement, AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = self.resolve_in(context, None).and_then(|mut merge| {
            if context.parameter_inference_needed() {
                let constraints = ParameterResolution::infer_merge(&mut merge, context, cache)?;
                context.set_parameter_constraints(&constraints);
            }
            Ok(merge)
        });
        context.finish_analysis(result)
    }
    pub(crate) fn require_version(
        context: &impl ParseContext,
        minimum: u32,
    ) -> Result<(), AnalysisError> {
        MergeStatement::require_version(context, minimum)
    }
    pub(crate) fn resolve_in(
        &self,
        context: &mut impl ParseContext,
        outer: Option<&ScalarAnalysisContext<'_>>,
    ) -> Result<MergeStatement, AnalysisError> {
        Self::require_version(context, 15)?;
        if self.ast.with_clause.recursive {
            return Err(AnalysisError::unexpected_input_error(
                "WITH RECURSIVE is not supported for MERGE",
            ));
        }
        if !self.ast.returning_list.is_empty() {
            Self::require_version(context, 17)?;
        }
        let requested = TableName::from(convert(&self.ast.relation.name).full_name()?);
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
            Self::require_version(context, 17)?;
        }
        let alias = self
            .ast
            .relation
            .alias
            .as_ref()
            .map(|value| convert(value).resolve())
            .unwrap_or_else(|| AliasSpec::from_local_name(&requested.full_name().local_name()));
        let with = convert(&self.ast.with_clause).resolve(context, outer)?;
        let target = DataSource::resolve_relation(
            context,
            convert(&table.name()).full_name(),
            &alias,
            &WithClause::default(),
        )?;
        let source = convert(&self.ast.source_relation).resolve(
            context,
            &with,
            &FromClause::default(),
            None,
        )?;
        if source.namespace().contains_alias(alias.name()) {
            context.report_analysis_problem(AnalysisProblem::duplicate_alias(alias.name()))?;
        }
        let on_scope = ScalarAnalysisContext::for_merge_on(&with, &target, &source);
        let on = convert(&self.ast.join_condition).resolve_merge_predicate(context, &on_scope)?;
        let clauses = self.resolve_clauses(context, &with, &target, &source, &table, &alias)?;
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
            &self.ast.returning_list,
            context,
            &returning_scope,
        )?;
        let mut statement = MergeStatement::new(
            table.name(),
            &convert(&self.ast.with_clause).local_definitions(with.clone()),
            on_scope.sources(),
            &on,
            &clauses,
            &returning,
            self.ast.relation.inherit,
        );
        statement.set_view_support(table.view_support());
        statement.validate_assignments(context)?;
        Ok(statement)
    }
    pub(crate) fn resolve_clauses(
        &self,
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
        for clause in &self.ast.merge_when_clauses {
            let index = convert(&clause.match_kind).index();
            if terminal[index] {
                return Err(AnalysisError::unexpected_input_error(
                    "unreachable MERGE WHEN clause after unconditional action",
                ));
            }
            terminal[index] = clause.condition.is_none();
            let kind = convert(&clause.match_kind).resolve(context)?;
            let scope = ScalarAnalysisContext::for_merge_when(with, target, source, kind);
            let condition = clause
                .condition
                .as_ref()
                .map(|expr| convert(expr).resolve_merge_predicate(context, &scope))
                .transpose()?;
            let action =
                convert(clause).resolve_action(context, &scope, &target_scope, table, alias)?;
            clauses.push(&SemMergeWhen::new(kind, &condition, &action));
        }
        Ok(clauses)
    }
}

#[cfg(test)]
mod tests;
impl AstConversion<'_, MergeMatchKind> {
    pub(crate) const fn index(self) -> usize {
        match self.ast {
            MergeMatchKind::Matched => 0,
            MergeMatchKind::NotMatchedBySource => 1,
            MergeMatchKind::NotMatchedByTarget => 2,
        }
    }
    pub(crate) fn resolve(
        self,
        context: &impl ParseContext,
    ) -> Result<SemMergeMatchKind, AnalysisError> {
        match self.ast {
            MergeMatchKind::Matched => Ok(SemMergeMatchKind::matched()),
            MergeMatchKind::NotMatchedByTarget => Ok(SemMergeMatchKind::by_target()),
            MergeMatchKind::NotMatchedBySource => SemMergeMatchKind::from_ast_by_source(context),
        }
    }
    pub(crate) const fn permits(self, command: CmdType) -> bool {
        match command {
            CmdType::Nothing => true,
            CmdType::Insert => matches!(self.ast, MergeMatchKind::NotMatchedByTarget),
            CmdType::Update | CmdType::Delete => {
                !matches!(self.ast, MergeMatchKind::NotMatchedByTarget)
            }
            _ => false,
        }
    }
}
impl SemMergeMatchKind {
    fn from_ast_by_source(context: &impl ParseContext) -> Result<Self, AnalysisError> {
        AstConversion::<Merge>::require_version(context, 17)?;
        Ok(Self::by_source())
    }
}

impl AstConversion<'_, MergeWhen> {
    pub(crate) fn resolve_action(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        target_scope: &ScalarAnalysisContext<'_>,
        target: &MergeDestination,
        alias: &AliasSpec,
    ) -> Result<MergeAction, AnalysisError> {
        if !convert(&self.ast.match_kind).permits(self.ast.command_type) {
            return Err(AnalysisError::unexpected_input_error(
                "illegal MERGE action for WHEN match kind",
            ));
        }
        match self.ast.command_type {
            CmdType::Update => {
                self.resolve_update_action(context, scope, target_scope, target, alias)
            }
            CmdType::Insert => {
                self.resolve_insert_action(context, scope, target_scope, target, alias)
            }
            CmdType::Delete => Ok(MergeAction::delete()),
            CmdType::Nothing => Ok(MergeAction::nothing()),
            _ => Err(AnalysisError::unexpected_input_error(
                "unsupported MERGE action",
            )),
        }
    }
    pub(crate) fn resolve_update_action(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        target_scope: &ScalarAnalysisContext<'_>,
        target: &MergeDestination,
        alias: &AliasSpec,
    ) -> Result<MergeAction, AnalysisError> {
        if !target.update().is_writable() {
            return Err(AnalysisError::unexpected_input_error(
                "MERGE target does not accept UPDATE assignments",
            ));
        }
        let result = UpdateAssignmentCollection::resolve(
            context,
            &self.ast.target_list,
            target.update(),
            alias,
            target_scope,
            scope,
        );
        let Some(assignments) = ResolutionFailure::finish(result, context)? else {
            return Ok(MergeAction::unresolved());
        };
        Ok(MergeAction::update(&assignments))
    }
    pub(crate) fn resolve_insert_action(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        target_scope: &ScalarAnalysisContext<'_>,
        target: &MergeDestination,
        alias: &AliasSpec,
    ) -> Result<MergeAction, AnalysisError> {
        if !target.insert().is_writable() {
            return Err(AnalysisError::unexpected_input_error(
                "MERGE target does not accept INSERT assignments",
            ));
        }
        let values = if self.ast.values.is_empty() {
            None
        } else {
            Some(self.ast.values.as_slice())
        };
        let result = InsertClauseSemantics::resolve(
            context,
            &self.ast.target_list,
            &InsertTargetBinding::new(target.insert(), alias, target_scope),
            values,
            scope,
            Some(&self.ast.overriding),
        );
        let Some(clause) = ResolutionFailure::finish(result, context)? else {
            return Ok(MergeAction::unresolved());
        };
        let overriding = convert(&self.ast.overriding).merge_override();
        Ok(MergeAction::insert(&clause, overriding.as_deref()))
    }
}
impl AstConversion<'_, OverridingKind> {
    pub(crate) fn merge_override(self) -> Option<String> {
        match self.ast {
            OverridingKind::NotSet => None,
            OverridingKind::UserValue => Some("user".to_owned()),
            OverridingKind::SystemValue => Some("system".to_owned()),
        }
    }
}
impl AstConversion<'_, Expr> {
    pub(crate) fn resolve_merge_predicate(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let value = self.resolve_in(context, scope)?;
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
                let qualifier = convert(name).star_qualifier()?;
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
