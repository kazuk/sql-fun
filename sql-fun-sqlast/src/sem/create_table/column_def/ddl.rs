//! Analysis owned by the semantic definitions in this module.

use crate::{
    ast::{Column as AstColumn, ColumnConstraint as AstColumnConstraint, Dialect as AstDialect},
    context::ParseContext,
    names::{ColumnName, FullName, TableName},
    sem::{
        AnalysisError, AnalysisProblem, CheckColumnConstraint, CheckConstraint, ColumnConstraint,
        ColumnDefinition, ConstraintNames, CreateTable, ScalarAnalysisContext, TypeReference,
        WithTypmod, create_table::serial::SerialSequences,
    },
};

impl ColumnDefinition {
    pub(crate) fn from_ast_and_report<TContext: ParseContext>(
        ast: &AstColumn,
        mut context: TContext,
        table: &TableName,
    ) -> Result<(Self, TContext), AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = Self::from_ast_standalone(ast, &mut context, table);
        let column = context.finish_analysis(result)?;
        Ok((column, context))
    }

    pub(crate) fn from_ast_standalone(
        ast: &AstColumn,
        context: &mut impl ParseContext,
        table: &TableName,
    ) -> Result<Self, AnalysisError> {
        let mut column = Self::from_ast(ast, context, table)?;
        let canonical = context
            .resolve_table_name(table)
            .unwrap_or_else(|| table.clone());
        let mut names = ConstraintNames::default();
        ConstraintNames::reserve_ast_column(ast, context, &mut names);
        Self::resolve_ast_inline_checks(ast, context, &canonical, None, &mut names, &mut column)?;
        Ok(column)
    }

    pub(crate) fn from_ast(
        ast: &AstColumn,
        context: &mut impl ParseContext,
        table: &TableName,
    ) -> Result<Self, AnalysisError> {
        let name = ColumnName::from(ast.name().as_str());
        let type_ref = Self::type_from_ast(ast, context, table, &name)?;
        let mut column = Self::new(&Some(name.clone()), type_ref.as_ref(), ast.not_null(), &[]);
        Self::retain_ast_declaration(ast, context, &mut column)?;
        Self::resolve_ast_constraints(ast, context, table, &name, &mut column)?;
        Ok(column)
    }

    pub(crate) fn type_from_ast(
        ast: &AstColumn,
        context: &mut impl ParseContext,
        table: &TableName,
        name: &ColumnName,
    ) -> Result<Option<TypeReference>, AnalysisError> {
        let type_ref = if let Some(ty) = &ast.type_name() {
            let type_name = FullName::from_ast_name(ty.name())?;
            if let Some(ty) = context.get_type(&type_name) {
                if context.dialect() == AstDialect::sqlite() {
                    Some(TypeReference::from_full_name(type_name))
                } else if let Some(type_ref) = ty.type_reference() {
                    let array = ast
                        .type_name()
                        .as_ref()
                        .is_some_and(|syntax| !syntax.array_bounds().is_empty());
                    Some(TypeReference::concrete_type_ref(
                        type_ref
                            .full_name()
                            .expect("catalog declaration has a nominal type identity"),
                        array,
                    ))
                } else {
                    let problem = AnalysisProblem::unexpected_dynamic_type(ty);
                    context.report_analysis_problem(problem)?;
                    None
                }
            } else {
                context.report_analysis_problem(AnalysisProblem::column_type_not_found(
                    table, name, &type_name,
                ))?;
                None
            }
        } else {
            None
        };
        Ok(type_ref)
    }

    pub(crate) fn retain_ast_declaration(
        ast: &AstColumn,
        context: &mut impl ParseContext,
        column: &mut Self,
    ) -> Result<(), AnalysisError> {
        if context.dialect() == AstDialect::postgres() {
            if let Some(type_name) = &ast.type_name() {
                column.retain_requested_type(&WithTypmod::<TypeReference>::from_ast_declaration(
                    type_name,
                    column.get_type(),
                )?);
            }
            let lookup = FullName::from_optional_ast_name(ast.collation())?
                .map(|name| context.lookup_collation(&name));
            column.validate_collation(context, lookup.as_ref())?;
        }
        Ok(())
    }

    pub(crate) fn resolve_ast_constraints(
        ast: &AstColumn,
        context: &mut impl ParseContext,
        table: &TableName,
        name: &ColumnName,
        column: &mut Self,
    ) -> Result<(), AnalysisError> {
        for constraint in ast.constraints() {
            // CHECK needs the complete row, which is declared before expression analysis.
            if constraint.is_check() {
                continue;
            }
            // ENFORCED is the existing constraint behavior. Retain its explicit
            // syntax, but it does not add a separate semantic column constraint.
            if matches!(
                constraint.constraint(),
                AstColumnConstraint::Enforcement(true)
            ) {
                continue;
            }
            if constraint.constraint().is_unique_key() && constraint.extras().deferrable() {
                todo!("deferrable unique-key catalog representation");
            }
            let constraint =
                ColumnConstraint::from_ast(constraint.constraint(), context, table, name)?;
            if let Some(nullability) = constraint.as_nullability() {
                column.set_not_null(Some(nullability.is_not_null()));
            }
            if context.dialect() == AstDialect::sqlite() {
                if let Some(default) = constraint.as_default() {
                    column.set_default(default.expr());
                }
            } else if let Some(fk) = constraint.as_foreign_key()
                && let Some(col_type) = column.get_type()
                && let Some(pk_table) = fk.primary_key_table()
                && let Some(pk_type) = fk.primary_key_column_type()
                && col_type != pk_type
            {
                context.report_analysis_problem(
                    AnalysisProblem::foreign_key_column_type_mismatch(
                        table,
                        name,
                        col_type,
                        pk_table,
                        fk.primary_key_column(),
                        pk_type,
                    ),
                )?;
            }
            column.push_constraint(constraint);
        }
        Ok(())
    }
}

impl ColumnDefinition {
    pub(crate) fn from_ast_serial(
        ast: &AstColumn,
        context: &mut impl ParseContext,
        table: &TableName,
        temporary: bool,
        sequences: &mut SerialSequences,
    ) -> Result<Self, AnalysisError> {
        let Some(integer) = SerialSequences::integer_for_ast_column(ast, context) else {
            return Self::from_ast(ast, context, table);
        };
        let expanded = ast.serial_expansion(integer);
        let valid = SerialSequences::validate_ast_column(ast, context)?;
        let mut column = Self::from_ast(&expanded, context, table)?;
        if !valid {
            column.reject_declaration();
            return Ok(column);
        }
        let owner = TableName::from(context.creation_name(table.full_name(), temporary)?);
        let sequence_name = sequences.stage(context, &owner, ast.name(), temporary);
        let name = ColumnName::from(ast.name().as_str());
        let not_null = AstColumnConstraint::serial_not_null();
        column.push_constraint(ColumnConstraint::from_ast(
            &not_null, context, table, &name,
        )?);
        column.set_not_null(Some(true));
        let default = AstColumnConstraint::serial_default(&sequence_name);
        column.push_constraint(ColumnConstraint::from_ast(&default, context, table, &name)?);
        Ok(column)
    }
}

#[cfg(test)]
mod serial_catalog_tests {
    use crate::{
        context::{ParseContext, SequenceInfoRead, TableInfoRead},
        names::{ColumnName, FullName, TableName},
        sem::{PgBuiltInType, TypeReference},
        test_helpers::{ownership::apply_ownership_sql, serial},
    };

    #[rstest::rstest]
    #[case::smallserial(
        "smallserial",
        TypeReference::from_full_name(FullName::with_schema("pg_catalog", "int2"))
    )]
    #[case::serial2(
        "serial2",
        TypeReference::from_full_name(FullName::with_schema("pg_catalog", "int2"))
    )]
    #[case::serial("serial", PgBuiltInType::int4())]
    #[case::serial4("serial4", PgBuiltInType::int4())]
    #[case::bigserial("bigserial", PgBuiltInType::int8())]
    #[case::serial8("serial8", PgBuiltInType::int8())]
    #[case::quoted(r#""serial""#, PgBuiltInType::int4())]
    fn retains_serial_effects(
        #[case] spelling: &str,
        #[case] expected: TypeReference,
    ) -> testresult::TestResult {
        let pair = apply_ownership_sql(
            serial::context(),
            &format!("CREATE TABLE users(id {spelling})"),
        )?;
        let table = pair.ast().as_create_table().unwrap();
        assert!(table.declarations_can_apply());
        let column = table.find_column(&ColumnName::from("id"), false).unwrap();
        assert_eq!(column.get_type(), Some(&expected));
        assert_eq!(column.is_not_null(), Some(true));
        let default = column.serial_default_expression().unwrap();
        assert_eq!(
            default.function_dependency(),
            Some(&FullName::with_schema("pg_catalog", "nextval"))
        );
        assert!(table.primary_key_columns().is_empty());
        let context = pair.into_context();
        assert!(context.problems().is_empty(), "{:?}", context.problems());
        let sequence = context
            .get_sequence_impl(&FullName::with_schema("public", "users_id_seq"))
            .unwrap();
        assert_eq!(
            sequence.owned_by(),
            Some(&TableName::from(FullName::with_schema("public", "users")))
        );
        let identity = sequence.name().clone();
        let frozen = context.freeze();
        assert!(frozen.get_sequence_impl(&identity).is_some());
        Ok(())
    }

    #[rstest::rstest]
    #[case::nullable("serial NULL", "serial_nullable")]
    #[case::default("serial DEFAULT 1", "serial_default_conflict")]
    #[case::identity("serial GENERATED ALWAYS AS IDENTITY", "serial_identity_conflict")]
    #[case::array("serial[]", "serial_array")]
    #[case::modifier("serial(3)", "serial_modifier")]
    fn rejected_serial_does_not_publish_objects(
        #[case] declaration: &str,
        #[case] expected_code: &str,
    ) -> testresult::TestResult {
        let pair = apply_ownership_sql(
            serial::context(),
            &format!("CREATE TABLE users(id {declaration})"),
        )?;
        assert!(
            !pair
                .ast()
                .as_create_table()
                .unwrap()
                .declarations_can_apply()
        );
        let context = pair.into_context();
        assert!(
            context
                .get_table_impl(&FullName::with_schema("public", "users"))
                .is_none()
        );
        assert!(
            context
                .get_sequence_impl(&FullName::with_schema("public", "users_id_seq"))
                .is_none()
        );
        assert!(context.problems().iter().any(|p| p.code() == expected_code));
        let context = apply_ownership_sql(context, "CREATE TABLE good(id integer)")?.into_context();
        assert!(
            context
                .get_table_impl(&FullName::with_schema("public", "good"))
                .is_some()
        );
        Ok(())
    }

    #[test]
    fn skipped_creation_and_name_collisions_preserve_sequences() -> testresult::TestResult {
        let context = apply_ownership_sql(serial::context(), "CREATE TABLE users(id integer)")?
            .into_context();
        let context = apply_ownership_sql(context, "CREATE TABLE IF NOT EXISTS users(id serial)")?
            .into_context();
        assert!(
            context
                .get_sequence_impl(&FullName::with_schema("public", "users_id_seq"))
                .is_none()
        );
        let context = apply_ownership_sql(context, "CREATE SEQUENCE other_id_seq")?.into_context();
        let context = apply_ownership_sql(context, "CREATE TABLE other(id serial, second serial)")?
            .into_context();
        assert!(
            context
                .get_sequence_impl(&FullName::with_schema("public", "other_id_seq1"))
                .is_some()
        );
        assert!(
            context
                .get_sequence_impl(&FullName::with_schema("public", "other_second_seq"))
                .is_some()
        );
        Ok(())
    }
    #[test]
    fn index_conflict_leaves_no_serial_sequence() -> testresult::TestResult {
        let pair = apply_ownership_sql(
            serial::context(),
            "CREATE TABLE users(id serial CONSTRAINT users_id_seq PRIMARY KEY)",
        )?;
        let context = pair.into_context();
        assert!(
            context
                .get_table_impl(&FullName::with_schema("public", "users"))
                .is_none()
        );
        assert!(
            context
                .get_sequence_impl(&FullName::with_schema("public", "users_id_seq"))
                .is_none()
        );
        assert!(
            context
                .problems()
                .iter()
                .any(|p| p.code() == "conflicting_creation")
        );
        Ok(())
    }
}

impl ColumnDefinition {
    pub(crate) fn resolve_ast_inline_checks(
        ast: &AstColumn,
        context: &mut impl ParseContext,
        target: &TableName,
        table: Option<&CreateTable>,
        names: &mut ConstraintNames,
        column: &mut Self,
    ) -> Result<(), AnalysisError> {
        if !CheckColumnConstraint::has_ast_column_checks(ast) {
            return Ok(());
        }
        let scope = if let Some(table) = table {
            ScalarAnalysisContext::declared_table(table, target)
        } else {
            ScalarAnalysisContext::column_declaration(context, target, column)
        };
        let name = ColumnName::from(ast.name().as_str());
        let mut index = 0;
        for definition in ast.constraints() {
            if definition.is_check() {
                let mut check = CheckConstraint::from_ast_expression(
                    definition, context, target, &scope, false,
                )?;
                if !names.reserve(check.name()) {
                    check.reject_duplicate_name(context)?;
                }
                let check = CheckColumnConstraint::from_analyzed(target, &name, &check);
                column.insert_inline_check(index, &ColumnConstraint::check(check));
            }
            if !matches!(definition.constraint(), AstColumnConstraint::Enforcement(_)) {
                index += 1;
            }
        }
        Ok(())
    }
}
