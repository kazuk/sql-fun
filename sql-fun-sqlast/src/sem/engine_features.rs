//! Validate engine feature gates before semantic interpretation and catalog changes.

use crate::ast::{
    AlterRelation, Column, CteBody, DefaultPrivileges, Delete, ForeignTable, Insert, Merge,
    PrivilegeObject, StatementNode, Table, TableConstraint, Update, With,
};
use crate::{
    ast::{ColumnConstraint, ConstraintExtras, Dialect, Query, ReturningOption},
    context::ParseContext,
    engine_specific::{Postgres18Feature, Postgres18Syntax},
    sem::{AnalysisError, GeneratedColumnMode},
};

/// Validates engine feature availability before semantic interpretation.
pub(crate) struct EngineFeatureValidation;

impl EngineFeatureValidation {
    pub(crate) fn returning_options(
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

    pub(crate) fn statement(
        ast: &StatementNode,
        context: &mut impl ParseContext,
    ) -> Result<bool, AnalysisError> {
        if context.dialect() != Dialect::postgres() {
            return Ok(true);
        }
        match ast {
            StatementNode::DefaultPrivileges(value) => Self::default_privileges(value, context),
            StatementNode::ForeignTable(value) => Self::foreign_table(value, context),
            StatementNode::Query(query) => Self::query(query, context),
            StatementNode::Insert(insert) => Self::insert(insert, context),
            StatementNode::Update(update) => Self::update(update, context),
            StatementNode::Delete(delete) => Self::delete(delete, context),
            StatementNode::Merge(merge) => Self::merge(merge, context),
            StatementNode::Table(table) => Self::table(table, context),
            StatementNode::Domain(domain) => Self::domain(domain, context),
            StatementNode::AlterRelation(alter) => Self::alter_relation(alter, context),
            _ => Ok(true),
        }
    }
}

#[cfg(test)]
mod utility_version_tests {
    use crate::sem::EngineFeatureValidation;

    use crate::sem::SemAst;

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
            EngineFeatureValidation::statement(&statement.node, &mut context).unwrap(),
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
    use crate::sem::EngineFeatureValidation;

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
        let _ = EngineFeatureValidation::statement(&statement.node, &mut context);
    }
}

impl EngineFeatureValidation {
    pub(crate) fn with_clause(
        ast: &With,
        context: &mut impl ParseContext,
    ) -> Result<bool, AnalysisError> {
        let mut valid = true;
        for entry in &ast.entries {
            valid &= Self::cte_body(&entry.body, context)?;
        }
        Ok(valid)
    }

    pub(crate) fn cte_body(
        ast: &CteBody,
        context: &mut impl ParseContext,
    ) -> Result<bool, AnalysisError> {
        match ast {
            CteBody::Query(query) => Self::query(query, context),
            CteBody::Insert(insert) => Self::insert(insert, context),
            CteBody::Update(update) => Self::update(update, context),
            CteBody::Delete(delete) => Self::delete(delete, context),
            CteBody::Merge(merge) => Self::merge(merge, context),
        }
    }

    pub(crate) fn query(
        ast: &Query,
        context: &mut impl ParseContext,
    ) -> Result<bool, AnalysisError> {
        Self::with_clause(&ast.with, context)
    }

    pub(crate) fn table_constraint(
        ast: &TableConstraint,
        context: &mut impl ParseContext,
        column: bool,
    ) -> Result<bool, AnalysisError> {
        if context.dialect() != Dialect::postgres() {
            return Ok(true);
        }
        let mut valid = Self::temporal_constraint(ast, context)?;
        if let Some(enforced) = ast.extras.enforced {
            valid &= Self::enforcement(context, enforced)?;
        }
        valid &= Self::column_constraint(&ast.constraint, context, &ast.extras, column)?;
        Ok(valid)
    }

    pub(crate) fn temporal_constraint(
        ast: &TableConstraint,
        context: &mut impl ParseContext,
    ) -> Result<bool, AnalysisError> {
        if !ast.extras.without_overlaps && !ast.extras.fk_with_period && !ast.extras.pk_with_period
        {
            return Ok(true);
        }
        if !context.postgres18_syntax_used(Postgres18Syntax::TemporalConstraint)? {
            return Ok(false);
        }
        todo!("temporal constraint analysis (#887)")
    }

    pub(crate) fn column_constraint(
        ast: &ColumnConstraint,
        context: &mut impl ParseContext,
        extras: &ConstraintExtras,
        column: bool,
    ) -> Result<bool, AnalysisError> {
        match ast {
            ColumnConstraint::Enforcement(enforced) => Self::enforcement(context, *enforced),
            ColumnConstraint::Generated(_, Some(mode)) if *mode == GeneratedColumnMode::Virtual => {
                context.postgres18_syntax_used(Postgres18Syntax::VirtualGeneratedColumn)
            }
            ColumnConstraint::Nullability(true, _, _)
                if !column || extras.no_inherit || extras.skip_validation =>
            {
                Self::not_null(context)
            }
            _ => Ok(true),
        }
    }

    pub(crate) fn enforcement(
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

    pub(crate) fn not_null(context: &mut impl ParseContext) -> Result<bool, AnalysisError> {
        if !context.postgres18_syntax_used(Postgres18Syntax::NotNullConstraint)? {
            return Ok(false);
        }
        todo!("PostgreSQL 18 NOT NULL constraint analysis (#887)")
    }

    pub(crate) fn column(
        ast: &Column,
        context: &mut impl ParseContext,
    ) -> Result<bool, AnalysisError> {
        let mut valid = true;
        for constraint in &ast.constraints {
            valid &= Self::table_constraint(constraint, context, true)?;
        }
        Ok(valid)
    }

    pub(crate) fn table(
        ast: &Table,
        context: &mut impl ParseContext,
    ) -> Result<bool, AnalysisError> {
        let mut valid = true;
        for column in &ast.columns {
            valid &= Self::column(column, context)?;
        }
        for constraint in &ast.constraints {
            valid &= Self::table_constraint(constraint, context, false)?;
        }
        Ok(valid)
    }
}

#[cfg(all(test, feature = "sqlite"))]
mod sqlite_feature_tests {
    use crate::sem::EngineFeatureValidation;
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
        assert!(EngineFeatureValidation::table(&table, &mut context).unwrap());
        assert_eq!(context.reported_problem_count(), 0);
    }
}

impl EngineFeatureValidation {
    pub(crate) fn alter_relation(
        ast: &AlterRelation,
        context: &mut impl ParseContext,
    ) -> Result<bool, AnalysisError> {
        if context.dialect() != Dialect::postgres() {
            return Ok(true);
        }
        let mut valid = true;
        for command in &ast.commands {
            valid &= Self::alter_command(command, context)?;
        }
        Ok(valid)
    }

    pub(crate) fn default_privileges(
        ast: &DefaultPrivileges,
        context: &mut impl ParseContext,
    ) -> Result<bool, AnalysisError> {
        if ast.object() == PrivilegeObject::large_objects() {
            context.postgres18_syntax_used(Postgres18Syntax::LargeObjectDefaultPrivileges)
        } else {
            Ok(true)
        }
    }

    pub(crate) fn foreign_table(
        ast: &ForeignTable,
        context: &mut impl ParseContext,
    ) -> Result<bool, AnalysisError> {
        if ast
            .elements()
            .iter()
            .any(|value| value.like_clause().is_some())
        {
            context.postgres18_syntax_used(Postgres18Syntax::ForeignTableLike)
        } else {
            Ok(true)
        }
    }

    pub(crate) fn insert(
        ast: &Insert,
        context: &mut impl ParseContext,
    ) -> Result<bool, AnalysisError> {
        check_dml_features(context, &ast.with, &ast.returning_options)
    }

    pub(crate) fn update(
        ast: &Update,
        context: &mut impl ParseContext,
    ) -> Result<bool, AnalysisError> {
        check_dml_features(context, &ast.with, &ast.returning_options)
    }

    pub(crate) fn delete(
        ast: &Delete,
        context: &mut impl ParseContext,
    ) -> Result<bool, AnalysisError> {
        check_dml_features(context, &ast.with, &ast.returning_options)
    }

    pub(crate) fn merge(
        ast: &Merge,
        context: &mut impl ParseContext,
    ) -> Result<bool, AnalysisError> {
        check_dml_features(context, &ast.with_clause, &ast.returning_options)
    }
}

fn check_dml_features(
    context: &mut impl ParseContext,
    with: &With,
    returning: &[ReturningOption],
) -> Result<bool, AnalysisError> {
    Ok(EngineFeatureValidation::with_clause(with, context)?
        & EngineFeatureValidation::returning_options(returning, context)?)
}
