//! Version notifications for retained PostgreSQL syntax, before catalog effects.
use crate::{
    ast::{
        ColumnConstraint, ConstraintExtras, CteBody, Query, ReturningOption, StatementNode,
        TableConstraint, With,
    },
    context::ParseContext,
    engine_specific::{Postgres18Feature, Postgres18Syntax},
    sem::{AnalysisError, AnalysisProblem},
    syn::{self, Opt},
};
use ::sql_fun_core::IVec;

fn check_feature(
    context: &mut impl ParseContext,
    feature: Postgres18Syntax,
    implemented: bool,
) -> Result<bool, AnalysisError> {
    if !context.postgres18_syntax_used(feature)? {
        return Ok(false);
    }
    if !implemented {
        context.report_analysis_problem(AnalysisProblem::FeatureAnalysisUnsupported {
            feature: feature.description().to_owned(),
        })?;
    }
    Ok(implemented)
}

impl ReturningOption {
    fn check_features(
        options: &[Self],
        context: &mut impl ParseContext,
    ) -> Result<bool, AnalysisError> {
        if options.is_empty() {
            return Ok(true);
        }
        // Retain aliases for parse/edit/deparse. Destination row-image binding
        // for INSERT/UPDATE and renamed images is separate semantic work (#621).
        check_feature(context, Postgres18Syntax::ReturningAliases, false)
    }
}

impl StatementNode {
    pub(super) fn check_engine_features(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<bool, AnalysisError> {
        if context.dialect() != super::Dialect::postgres() {
            return Ok(true);
        }
        match self {
            Self::Query(query) => query.check_engine_features(context),
            Self::Insert(insert) => Ok(insert.with.check_engine_features(context)?
                & ReturningOption::check_features(&insert.returning_options, context)?),
            Self::Update(update) => Ok(update.with.check_engine_features(context)?
                & ReturningOption::check_features(&update.returning_options, context)?),
            Self::Delete(delete) => Ok(delete.with.check_engine_features(context)?
                & ReturningOption::check_features(&delete.returning_options, context)?),
            Self::Merge(merge) => Ok(merge.with_clause.check_engine_features(context)?
                & ReturningOption::check_features(&merge.returning_options, context)?),
            Self::Table(table) => table.check_engine_features(context),
            Self::Domain(domain) => domain.check_engine_features(context),
            Self::AlterRelation(alter) => alter.check_engine_features(context),
            _ => Ok(true),
        }
    }
}

impl With {
    fn check_engine_features(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<bool, AnalysisError> {
        let mut valid = true;
        for entry in &self.entries {
            valid &= entry.body.check_engine_features(context)?;
        }
        Ok(valid)
    }
}

impl CteBody {
    pub(in crate::ast) fn check_engine_features(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<bool, AnalysisError> {
        match self {
            Self::Query(query) => query.check_engine_features(context),
            Self::Insert(insert) => Ok(insert.with.check_engine_features(context)?
                & ReturningOption::check_features(&insert.returning_options, context)?),
            Self::Update(update) => Ok(update.with.check_engine_features(context)?
                & ReturningOption::check_features(&update.returning_options, context)?),
            Self::Delete(delete) => Ok(delete.with.check_engine_features(context)?
                & ReturningOption::check_features(&delete.returning_options, context)?),
            Self::Merge(merge) => Ok(merge.with_clause.check_engine_features(context)?
                & ReturningOption::check_features(&merge.returning_options, context)?),
        }
    }
}

impl Query {
    fn check_engine_features(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<bool, AnalysisError> {
        self.with.check_engine_features(context)
    }
}

impl TableConstraint {
    pub(super) fn check_engine_features(
        &self,
        context: &mut impl ParseContext,
        column: bool,
    ) -> Result<bool, AnalysisError> {
        let mut valid = true;
        if self.extras.without_overlaps || self.extras.fk_with_period || self.extras.pk_with_period
        {
            valid &= check_feature(context, Postgres18Syntax::TemporalConstraint, false)?;
        }
        if let Some(enforced) = self.extras.enforced {
            valid &= check_feature(context, Postgres18Syntax::ConstraintEnforcement, enforced)?;
        }
        match &self.constraint {
            ColumnConstraint::Enforcement(enforced) => {
                valid &=
                    check_feature(context, Postgres18Syntax::ConstraintEnforcement, *enforced)?;
            }
            ColumnConstraint::Generated(_, Some(mode))
                if *mode == crate::sem::GeneratedColumnMode::Virtual =>
            {
                valid &= check_feature(context, Postgres18Syntax::VirtualGeneratedColumn, true)?;
            }
            ColumnConstraint::Nullability(true, _, _)
                if !column || self.extras.no_inherit || self.extras.skip_validation =>
            {
                valid &= check_feature(context, Postgres18Syntax::NotNullConstraint, false)?;
            }
            _ => {}
        }
        Ok(valid)
    }
}

impl ConstraintExtras {
    pub(super) fn explicit_enforcement(
        value: &syn::Constraint,
        tokens: &IVec<syn::ScanToken>,
    ) -> Option<bool> {
        if !matches!(
            value.get_contype().as_inner(),
            Some(syn::ConstrType::ConstrCheck | syn::ConstrType::ConstrForeign)
        ) {
            return None;
        }
        let mut depth = 0;
        for token in tokens
            .iter()
            .filter(|token| token.get_start() >= value.get_location())
        {
            match token.get_token().as_inner() {
                Some(syn::Token::Ascii40) => depth += 1,
                Some(syn::Token::Ascii41) if depth == 0 => break,
                Some(syn::Token::Ascii41) => depth -= 1,
                Some(syn::Token::Ascii44) if depth == 0 => break,
                Some(syn::Token::Enforced) if depth == 0 => return Some(value.get_is_enforced()),
                _ => {}
            }
        }
        None
    }
}

impl super::Table {
    pub(super) fn check_engine_features(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<bool, AnalysisError> {
        let mut valid = true;
        for column in &self.columns {
            for constraint in &column.constraints {
                valid &= constraint.check_engine_features(context, true)?;
            }
        }
        for constraint in &self.constraints {
            valid &= constraint.check_engine_features(context, false)?;
        }
        Ok(valid)
    }
}

impl super::AlterRelation {
    pub(super) fn check_engine_features(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<bool, AnalysisError> {
        let mut valid = true;
        for command in &self.commands {
            valid &= command.check_engine_features(context)?;
        }
        Ok(valid)
    }
}
