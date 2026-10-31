//! Version notifications for retained PostgreSQL syntax, before catalog effects.
use crate::{
    ast::{
        ColumnConstraint, ConstraintExtras, CteBody, Query, ReturningOption, StatementNode,
        TableConstraint, With,
    },
    context::ParseContext,
    engine_specific::{Postgres18Feature, Postgres18Syntax},
    sem::AnalysisError,
    syn::{self, Opt},
};
use ::sql_fun_core::IVec;

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
        if !context.postgres18_syntax_used(Postgres18Syntax::ReturningAliases)? {
            return Ok(false);
        }
        todo!("RETURNING row-image aliases (#621)")
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

#[cfg(test)]
mod unimplemented_analysis_tests {
    #[rstest::rstest]
    #[case::insert_alias("INSERT INTO t VALUES (1) RETURNING WITH (NEW AS n) n.id")]
    #[case::update_alias("UPDATE t SET id=1 RETURNING WITH (OLD AS o) o.id")]
    #[case::delete_alias("DELETE FROM t RETURNING WITH (OLD AS o) o.id")]
    #[case::merge_alias(
        "MERGE INTO t USING s ON true WHEN MATCHED THEN DELETE RETURNING WITH (OLD AS o) o.id"
    )]
    #[case::cte_alias("WITH c AS (DELETE FROM t RETURNING WITH (OLD AS o) o.id) SELECT * FROM c")]
    #[case::temporal("CREATE TABLE t (id int, p int4range, PRIMARY KEY (id,p WITHOUT OVERLAPS))")]
    #[case::non_enforced("CREATE TABLE t (id int CHECK (true) NOT ENFORCED)")]
    #[case::table_not_null("CREATE TABLE t (id int, NOT NULL id)")]
    #[case::alter_enforcement("ALTER TABLE t ALTER CONSTRAINT c NOT ENFORCED")]
    #[case::alter_inheritance("ALTER TABLE t ALTER CONSTRAINT c NO INHERIT")]
    #[case::domain("CREATE DOMAIN d AS int CHECK (true) ENFORCED")]
    #[should_panic(expected = "not yet implemented")]
    fn compatible_target_reaches_todo(#[case] sql: &str) {
        let mut context =
            crate::test_helpers::TestParseContext::default().with_postgres_version("18");
        let statement = crate::parse_statements(sql, crate::ast::Dialect::postgres())
            .next()
            .unwrap()
            .unwrap();
        let _ = statement.node.check_engine_features(&mut context);
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
        if context.dialect() != super::Dialect::postgres() {
            return Ok(true);
        }
        let mut valid = self.check_temporal_features(context)?;
        if let Some(enforced) = self.extras.enforced {
            valid &= ColumnConstraint::check_enforcement(context, enforced)?;
        }
        valid &= self
            .constraint
            .check_engine_features(context, &self.extras, column)?;
        Ok(valid)
    }

    fn check_temporal_features(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<bool, AnalysisError> {
        if !self.extras.without_overlaps
            && !self.extras.fk_with_period
            && !self.extras.pk_with_period
        {
            return Ok(true);
        }
        if !context.postgres18_syntax_used(Postgres18Syntax::TemporalConstraint)? {
            return Ok(false);
        }
        todo!("temporal constraint analysis (#887)")
    }
}

impl ColumnConstraint {
    fn check_engine_features(
        &self,
        context: &mut impl ParseContext,
        extras: &ConstraintExtras,
        column: bool,
    ) -> Result<bool, AnalysisError> {
        match self {
            Self::Enforcement(enforced) => Self::check_enforcement(context, *enforced),
            Self::Generated(_, Some(mode)) if *mode == crate::sem::GeneratedColumnMode::Virtual => {
                context.postgres18_syntax_used(Postgres18Syntax::VirtualGeneratedColumn)
            }
            Self::Nullability(true, _, _)
                if !column || extras.no_inherit || extras.skip_validation =>
            {
                Self::check_not_null(context)
            }
            _ => Ok(true),
        }
    }

    fn check_enforcement(
        context: &mut impl ParseContext,
        enforced: bool,
    ) -> Result<bool, AnalysisError> {
        if !context.postgres18_syntax_used(Postgres18Syntax::ConstraintEnforcement)? {
            return Ok(false);
        }
        if !enforced {
            todo!("non-enforced constraint analysis (#887)");
        }
        Ok(true)
    }

    fn check_not_null(context: &mut impl ParseContext) -> Result<bool, AnalysisError> {
        if !context.postgres18_syntax_used(Postgres18Syntax::NotNullConstraint)? {
            return Ok(false);
        }
        todo!("PostgreSQL 18 NOT NULL constraint analysis (#887)")
    }
}

impl super::Column {
    pub(super) fn check_engine_features(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<bool, AnalysisError> {
        let mut valid = true;
        for constraint in &self.constraints {
            valid &= constraint.check_engine_features(context, true)?;
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
            valid &= column.check_engine_features(context)?;
        }
        for constraint in &self.constraints {
            valid &= constraint.check_engine_features(context, false)?;
        }
        Ok(valid)
    }
}

#[cfg(all(test, feature = "sqlite"))]
mod sqlite_feature_tests {
    use crate::{ast::TryFromWithTokens, syn::Opt};

    #[test]
    fn virtual_column_has_no_postgres_version_requirement() {
        let parser = crate::test_helpers::SynBuilder::new();
        let raw = parser.parse("CREATE TABLE t (id int, v int GENERATED ALWAYS AS (1) VIRTUAL)");
        let table = super::super::Table::try_from_with_tokens(
            &raw.as_create_stmt().as_inner().unwrap(),
            &::sql_fun_core::IVec::default(),
        )
        .unwrap();
        let mut context = crate::test_helpers::TestParseContext::default()
            .with_dialect(crate::ast::Dialect::sqlite());
        assert!(table.check_engine_features(&mut context).unwrap());
        assert_eq!(context.reported_problem_count(), 0);
    }
}

impl super::AlterRelation {
    pub(super) fn check_engine_features(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<bool, AnalysisError> {
        if context.dialect() != super::Dialect::postgres() {
            return Ok(true);
        }
        let mut valid = true;
        for command in &self.commands {
            valid &= command.check_engine_features(context)?;
        }
        Ok(valid)
    }
}
