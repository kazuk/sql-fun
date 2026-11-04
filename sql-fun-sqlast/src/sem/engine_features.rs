//! Validate engine feature gates before semantic interpretation and catalog changes.

use crate::ast::ddl::Domain;
use crate::ast::{
    AlterRelation, Column, CteBody, DefaultPrivileges, Delete, ForeignTable, Insert, Merge,
    PrivilegeObject, StatementNode, Table, TableConstraint, Update, With,
};
use crate::sem::{AstConversion, convert};

use crate::{
    ast::{ColumnConstraint, ConstraintExtras, Dialect, Query, ReturningOption},
    context::ParseContext,
    engine_specific::{Postgres18Feature, Postgres18Syntax},
    sem::{AnalysisError, GeneratedColumnMode},
};

impl AstConversion<'_, ReturningOption> {
    pub(crate) fn check_features(
        options: &[ReturningOption],
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

impl AstConversion<'_, StatementNode> {
    pub(crate) fn check_engine_features(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<bool, AnalysisError> {
        if context.dialect() != Dialect::postgres() {
            return Ok(true);
        }
        match self.ast {
            StatementNode::DefaultPrivileges(value) => {
                convert::<DefaultPrivileges>(value).check_engine_features(context)
            }
            StatementNode::ForeignTable(value) => {
                convert::<ForeignTable>(value).check_engine_features(context)
            }
            StatementNode::Query(query) => convert::<Query>(query).check_engine_features(context),
            StatementNode::Insert(insert) => {
                convert::<Insert>(insert).check_engine_features(context)
            }
            StatementNode::Update(update) => {
                convert::<Update>(update).check_engine_features(context)
            }
            StatementNode::Delete(delete) => {
                convert::<Delete>(delete).check_engine_features(context)
            }
            StatementNode::Merge(merge) => convert::<Merge>(merge).check_engine_features(context),
            StatementNode::Table(table) => convert::<Table>(table).check_engine_features(context),
            StatementNode::Domain(domain) => {
                convert::<Domain>(domain).check_engine_features(context)
            }
            StatementNode::AlterRelation(alter) => {
                convert::<AlterRelation>(alter).check_engine_features(context)
            }
            _ => Ok(true),
        }
    }
}

#[cfg(test)]
mod utility_version_tests {
    use crate::sem::SemAst;

    use crate::sem::convert;
    use crate::{
        ast::Dialect,
        parse_statements,
        test_helpers::{TestParseContext, create_cache_context},
    };

    #[rstest::rstest]
    #[case::large_objects(
        "ALTER DEFAULT PRIVILEGES GRANT SELECT ON LARGE OBJECTS TO PUBLIC",
        true
    )]
    #[case::foreign_like("CREATE FOREIGN TABLE f (LIKE missing) SERVER absent", true)]
    #[case::older_privileges("ALTER DEFAULT PRIVILEGES GRANT SELECT ON TABLES TO PUBLIC", false)]
    #[case::older_foreign("CREATE FOREIGN TABLE f (x missing_type) SERVER absent", false)]
    fn syntax_versions_need_no_catalog(
        #[case] sql: &str,
        #[case] requires_18: bool,
        #[values("17", "18")] version: &str,
    ) {
        let statement = parse_statements(sql, Dialect::postgres())
            .next()
            .unwrap()
            .unwrap();
        let mut context = TestParseContext::default().with_postgres_version(version);
        assert_eq!(
            convert(&statement.node)
                .check_engine_features(&mut context)
                .unwrap(),
            !requires_18 || version == "18"
        );
        assert_eq!(
            context.reported_problem_count(),
            usize::from(requires_18 && version == "17")
        );
        assert_eq!(
            context.has_reported_problem("incompatible_engine_version"),
            requires_18 && version == "17"
        );
    }

    #[rstest::rstest]
    #[case("ALTER DEFAULT PRIVILEGES GRANT SELECT ON LARGE OBJECTS TO PUBLIC")]
    #[case("CREATE FOREIGN TABLE f (LIKE missing) SERVER absent")]
    fn incompatible_syntax_returns_before_unimplemented_interpretation(#[case] sql: &str) {
        let statement = parse_statements(sql, Dialect::postgres())
            .next()
            .unwrap()
            .unwrap();
        let context = TestParseContext::default().with_postgres_version("17");
        let result = SemAst::from_ast(&statement, context, &mut create_cache_context()).unwrap();
        assert!(
            result
                .context()
                .has_reported_problem("incompatible_engine_version")
        );
    }

    #[rstest::rstest]
    #[case::privileges("ALTER DEFAULT PRIVILEGES GRANT SELECT ON LARGE OBJECTS TO PUBLIC")]
    #[case::foreign("CREATE FOREIGN TABLE f (LIKE missing) SERVER absent")]
    #[should_panic(expected = "interpretation")]
    fn compatible_syntax_retains_semantic_todo(#[case] sql: &str) {
        let statement = parse_statements(sql, Dialect::postgres())
            .next()
            .unwrap()
            .unwrap();
        let context = TestParseContext::default().with_postgres_version("18");
        let _ = SemAst::from_ast(&statement, context, &mut create_cache_context());
    }
}

#[cfg(test)]
mod unimplemented_analysis_tests {
    use crate::sem::convert;
    use crate::{ast::Dialect, parse_statements, test_helpers::TestParseContext};

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
        let mut context = TestParseContext::default().with_postgres_version("18");
        let statement = parse_statements(sql, Dialect::postgres())
            .next()
            .unwrap()
            .unwrap();
        let _ = convert(&statement.node).check_engine_features(&mut context);
    }
}

impl AstConversion<'_, With> {
    pub(crate) fn check_engine_features(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<bool, AnalysisError> {
        let mut valid = true;
        for entry in &self.ast.entries {
            valid &= convert(&entry.body).check_engine_features(context)?;
        }
        Ok(valid)
    }
}

impl AstConversion<'_, CteBody> {
    pub(crate) fn check_engine_features(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<bool, AnalysisError> {
        match self.ast {
            CteBody::Query(query) => convert::<Query>(query).check_engine_features(context),
            CteBody::Insert(insert) => convert::<Insert>(insert).check_engine_features(context),
            CteBody::Update(update) => convert::<Update>(update).check_engine_features(context),
            CteBody::Delete(delete) => convert::<Delete>(delete).check_engine_features(context),
            CteBody::Merge(merge) => convert::<Merge>(merge).check_engine_features(context),
        }
    }
}

impl AstConversion<'_, Query> {
    pub(crate) fn check_engine_features(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<bool, AnalysisError> {
        convert(&self.ast.with).check_engine_features(context)
    }
}

impl AstConversion<'_, TableConstraint> {
    pub(crate) fn check_engine_features(
        &self,
        context: &mut impl ParseContext,
        column: bool,
    ) -> Result<bool, AnalysisError> {
        if context.dialect() != Dialect::postgres() {
            return Ok(true);
        }
        let mut valid = self.check_temporal_features(context)?;
        if let Some(enforced) = self.ast.extras.enforced {
            valid &= AstConversion::<ColumnConstraint>::check_enforcement(context, enforced)?;
        }
        valid &= convert(&self.ast.constraint).check_engine_features(
            context,
            &self.ast.extras,
            column,
        )?;
        Ok(valid)
    }

    pub(crate) fn check_temporal_features(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<bool, AnalysisError> {
        if !self.ast.extras.without_overlaps
            && !self.ast.extras.fk_with_period
            && !self.ast.extras.pk_with_period
        {
            return Ok(true);
        }
        if !context.postgres18_syntax_used(Postgres18Syntax::TemporalConstraint)? {
            return Ok(false);
        }
        todo!("temporal constraint analysis (#887)")
    }
}

impl AstConversion<'_, ColumnConstraint> {
    pub(crate) fn check_engine_features(
        &self,
        context: &mut impl ParseContext,
        extras: &ConstraintExtras,
        column: bool,
    ) -> Result<bool, AnalysisError> {
        match self.ast {
            ColumnConstraint::Enforcement(enforced) => Self::check_enforcement(context, *enforced),
            ColumnConstraint::Generated(_, Some(mode)) if *mode == GeneratedColumnMode::Virtual => {
                context.postgres18_syntax_used(Postgres18Syntax::VirtualGeneratedColumn)
            }
            ColumnConstraint::Nullability(true, _, _)
                if !column || extras.no_inherit || extras.skip_validation =>
            {
                Self::check_not_null(context)
            }
            _ => Ok(true),
        }
    }

    pub(crate) fn check_enforcement(
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

    pub(crate) fn check_not_null(context: &mut impl ParseContext) -> Result<bool, AnalysisError> {
        if !context.postgres18_syntax_used(Postgres18Syntax::NotNullConstraint)? {
            return Ok(false);
        }
        todo!("PostgreSQL 18 NOT NULL constraint analysis (#887)")
    }
}

impl AstConversion<'_, Column> {
    pub(crate) fn check_engine_features(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<bool, AnalysisError> {
        let mut valid = true;
        for constraint in &self.ast.constraints {
            valid &= convert(constraint).check_engine_features(context, true)?;
        }
        Ok(valid)
    }
}

impl AstConversion<'_, Table> {
    pub(crate) fn check_engine_features(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<bool, AnalysisError> {
        let mut valid = true;
        for column in &self.ast.columns {
            valid &= convert(column).check_engine_features(context)?;
        }
        for constraint in &self.ast.constraints {
            valid &= convert(constraint).check_engine_features(context, false)?;
        }
        Ok(valid)
    }
}

#[cfg(all(test, feature = "sqlite"))]
mod sqlite_feature_tests {
    use crate::sem::convert;
    use crate::{
        ast::{Dialect, Table},
        test_helpers::{SynBuilder, TestParseContext},
    };

    use crate::{ast::TryFromWithTokens, syn::Opt};

    #[test]
    fn virtual_column_has_no_postgres_version_requirement() {
        let parser = SynBuilder::new();
        let raw = parser.parse("CREATE TABLE t (id int, v int GENERATED ALWAYS AS (1) VIRTUAL)");
        let table = Table::try_from_with_tokens(
            &raw.as_create_stmt().as_inner().unwrap(),
            &::sql_fun_core::IVec::default(),
        )
        .unwrap();
        let mut context = TestParseContext::default().with_dialect(Dialect::sqlite());
        assert!(convert(&table).check_engine_features(&mut context).unwrap());
        assert_eq!(context.reported_problem_count(), 0);
    }
}

impl AstConversion<'_, AlterRelation> {
    pub(crate) fn check_engine_features(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<bool, AnalysisError> {
        if context.dialect() != Dialect::postgres() {
            return Ok(true);
        }
        let mut valid = true;
        for command in &self.ast.commands {
            valid &= convert(command).check_engine_features(context)?;
        }
        Ok(valid)
    }
}

impl AstConversion<'_, DefaultPrivileges> {
    pub(crate) fn check_engine_features(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<bool, AnalysisError> {
        if self.ast.object() == PrivilegeObject::large_objects() {
            context.postgres18_syntax_used(Postgres18Syntax::LargeObjectDefaultPrivileges)
        } else {
            Ok(true)
        }
    }
}

impl AstConversion<'_, ForeignTable> {
    pub(crate) fn check_engine_features(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<bool, AnalysisError> {
        if self
            .ast
            .elements()
            .iter()
            .any(|value| value.like_clause().is_some())
        {
            context.postgres18_syntax_used(Postgres18Syntax::ForeignTableLike)
        } else {
            Ok(true)
        }
    }
}

impl AstConversion<'_, Insert> {
    pub(crate) fn check_engine_features(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<bool, AnalysisError> {
        check_dml_features(context, &self.ast.with, &self.ast.returning_options)
    }
}

impl AstConversion<'_, Update> {
    pub(crate) fn check_engine_features(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<bool, AnalysisError> {
        check_dml_features(context, &self.ast.with, &self.ast.returning_options)
    }
}

impl AstConversion<'_, Delete> {
    pub(crate) fn check_engine_features(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<bool, AnalysisError> {
        check_dml_features(context, &self.ast.with, &self.ast.returning_options)
    }
}

impl AstConversion<'_, Merge> {
    pub(crate) fn check_engine_features(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<bool, AnalysisError> {
        check_dml_features(context, &self.ast.with_clause, &self.ast.returning_options)
    }
}

fn check_dml_features(
    context: &mut impl ParseContext,
    with: &With,
    returning: &[ReturningOption],
) -> Result<bool, AnalysisError> {
    Ok(convert(with).check_engine_features(context)?
        & AstConversion::<ReturningOption>::check_features(returning, context)?)
}
