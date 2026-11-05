//! Analysis owned by the semantic definitions in this module.

use crate::{
    AstAndContextPair,
    ast::{
        Dialect as AstDialect, DropTable as AstDropTable, Table as AstTable,
        TableConstraint as AstTableConstraint,
    },
    context::ParseContext,
    names::{ColumnName, FullName, TableName},
    sem::{
        AnalysisError, AnalysisProblem, CheckColumnConstraint, CheckConstraint, ColumnDefinition,
        Constraint, ConstraintNames, CreateTable, DropTable, EngineFeatureValidation, Kind,
        ScalarAnalysisContext, SemAst, SourcePathWithSpan, create_table::serial::SerialSequences,
        problem::CreationProblem,
    },
};

impl CreateTable {
    pub(crate) fn from_ast_and_apply<TContext: ParseContext>(
        ast: &AstTable,
        context: TContext,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError> {
        let Some(source) = context.get_source_path_with_span().cloned() else {
            return AnalysisError::raise_unexpected_none("source span");
        };
        Self::from_ast_and_apply_at(ast, context, &source)
    }

    pub(crate) fn constraint_names_from_ast(
        ast: &AstTable,
        context: &impl ParseContext,
    ) -> ConstraintNames {
        let mut names = ConstraintNames::default();
        for definition in ast.constraints() {
            if !definition.is_check() {
                names.reserve(definition.name().as_deref().unwrap_or_default());
            }
        }
        for column in ast.columns() {
            ConstraintNames::reserve_ast_column(column, context, &mut names);
        }
        names
    }

    pub(crate) fn from_ast_and_apply_at<TContext: ParseContext>(
        ast: &AstTable,
        mut context: TContext,
        source: &SourcePathWithSpan,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError> {
        if !EngineFeatureValidation::table(ast, &mut context)? {
            return Ok(AstAndContextPair::new(
                SemAst::unresolved_statement(),
                context,
            ));
        }
        let name = context.creation_name(&FullName::from_ast_name(ast.name())?, ast.temporary())?;
        if !context.permits_relation_creation(
            &name,
            Kind::table(),
            ast.mode().if_not_exists(),
            false,
        )? {
            return Ok(AstAndContextPair::new(
                SemAst::unresolved_statement(),
                context,
            ));
        }
        let mut names = Self::constraint_names_from_ast(ast, &context);
        if context.dialect() == AstDialect::postgres()
            && let Some(constraint) = names.first_duplicate()
        {
            context.report_problem(AnalysisProblem::creation(CreationProblem::constraint(
                &name,
                &FullName::from_local_name(constraint),
                source.span(),
            )))?;
            return Ok(AstAndContextPair::new(
                SemAst::unresolved_statement(),
                context,
            ));
        }
        let table = Self::from_ast_with_names(ast, &mut context, source, &mut names)?;
        let context = context.apply_create_table(&table)?;
        Ok(AstAndContextPair::new(
            SemAst::create_table(&table),
            context,
        ))
    }
}

#[cfg(all(test, feature = "sqlite"))]
#[cfg(test)]
mod test_analyze_create_table {
    use crate::{
        AstAndContextPair,
        ast::{CreateTableMode, Dialect},
        context::BaseParseContext,
        names::{FullName, SchemaName, TableName},
        sem::{SemAst, SourcePathWithSpan, create_table::CreateTable as CreateTableCreateTable},
        syn::sqlite::Stmt,
        test_helpers::{
            SqliteSynBuilder, TestParseContext, sem_builder, source_span, sqlite_syn, test_context,
        },
    };

    use assert_matches::assert_matches;

    use testresult::TestResult;

    #[rstest::rstest]
    #[case(
        "create table users(id integer, name text)",
        "users",
        CreateTableMode::Default,
        false
    )]
    #[case(
        "create table if not exists users(id integer, name text)",
        "users",
        CreateTableMode::IfNotExists,
        false
    )]
    #[case(
        "create temp table if not exists temp_test(id integer, name text)",
        "temp_test",
        CreateTableMode::IfNotExists,
        true
    )]
    fn test_analyze_create_table(
        test_context: TestParseContext,
        sqlite_syn: SqliteSynBuilder,
        source_span: SourcePathWithSpan,
        #[case] sql: &str,
        #[case] table_name: &str,
        #[case] table_mode: CreateTableMode,
        #[case] temporary: bool,
    ) -> TestResult {
        let sem_builder = sem_builder();
        let mut test_context = test_context.with_dialect(Dialect::sqlite());
        test_context.set_get_search_path_result(&[SchemaName::from("main")]);

        let stmt = sqlite_syn.parse_as_stmt(sql);

        let Stmt::CreateTable(ct) = stmt else {
            panic!("expect CreateTable: {stmt:?}");
        };
        let tokens = Vec::default().into();
        let AstAndContextPair(sem_ast, result_context) =
            CreateTableCreateTable::analyze_create_table(
                test_context,
                &None,
                ct,
                &tokens,
                &source_span,
            )?;
        assert_matches!(sem_ast, SemAst::CreateTable(_));
        let Some(create_table) = sem_ast.as_create_table() else {
            panic!("expect CreateTable")
        };
        assert_eq!(create_table.name(), &sem_builder.table_name(table_name));
        assert_eq!(create_table.mode(), &table_mode);
        assert_eq!(create_table.temporary(), temporary);

        let lookup_name = if temporary {
            TableName::from(FullName::with_schema("temp", table_name))
        } else {
            sem_builder.table_name(table_name)
        };
        let Some(_table_in_context) = result_context.get_table(&lookup_name) else {
            panic!("table not found in context")
        };

        Ok(())
    }
}

#[cfg(test)]
mod test_skipped_creation_and_name_collisions_preserve_sequences {
    use crate::{
        context::SequenceInfoRead,
        names::FullName,
        test_helpers::{ownership::apply_ownership_sql, serial},
    };

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
}

impl CreateTable {
    #[cfg(any(test, feature = "sqlite"))]
    pub(crate) fn from_ast(
        ast: &AstTable,
        context: &mut impl ParseContext,
        source: &SourcePathWithSpan,
    ) -> Result<Self, AnalysisError> {
        let mut names = Self::constraint_names_from_ast(ast, context);
        Self::from_ast_with_names(ast, context, source, &mut names)
    }

    pub(crate) fn from_ast_with_names(
        ast: &AstTable,
        context: &mut impl ParseContext,
        source: &SourcePathWithSpan,
        names: &mut ConstraintNames,
    ) -> Result<Self, AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = Self::from_ast_inner(ast, context, source, names);
        context.finish_analysis(result)
    }

    pub(crate) fn from_ast_inner(
        ast: &AstTable,
        context: &mut impl ParseContext,
        source: &SourcePathWithSpan,
        names: &mut ConstraintNames,
    ) -> Result<Self, AnalysisError> {
        if ast.query().is_some() {
            todo!()
        }
        let name = TableName::from(FullName::from_ast_name(ast.name())?);
        let mut columns = Vec::new();
        let mut serials = SerialSequences::default();
        for column in ast.columns() {
            columns.push(ColumnDefinition::from_ast_serial(
                column,
                context,
                &name,
                ast.temporary(),
                &mut serials,
            )?);
        }
        let mut table = Self::new(&name, columns, ast.mode(), ast.temporary(), source);
        table.retain_serial_sequences(serials.into_sequences());
        for definition in ast.constraints() {
            if let Some(constraint) =
                Constraint::from_ast_declared_key(definition, context, &table)?
            {
                table.record_declared_constraint(&constraint)?;
            }
        }
        for column in ast.columns() {
            for definition in column.constraints() {
                if definition.name().is_some() && definition.constraint().is_unique_key() {
                    let keys = [ColumnName::from(column.name().as_str())];
                    let constraint = Constraint::from_ast_column_table(
                        definition.constraint(),
                        context,
                        &name,
                        definition.name().as_deref().expect("named inline key"),
                        &keys,
                    )?;
                    table.record_declared_constraint(&constraint)?;
                }
            }
        }
        Self::resolve_ast_foreign_keys(ast, context, &mut table)?;
        if context.dialect() == AstDialect::postgres() {
            table.apply_primary_key_nullability();
        }
        Self::resolve_ast_inline_checks(ast, context, &mut table, names)?;
        Self::resolve_ast_checks(ast, context, &mut table, names)?;
        Ok(table)
    }
}

#[cfg(all(test, feature = "sqlite"))]
#[cfg(test)]
mod test_preserves_explicit_creation_mode {
    use crate::{
        ast::CreateTableMode,
        names::SchemaName,
        sem::{SourcePathWithSpan, create_table::CreateTable},
        syn::sqlite::{CreateTableBody, Stmt},
        test_helpers::{
            SqliteSynBuilder, TestParseContext, sem_builder, source_span, sqlite_syn, test_context,
        },
    };

    #[rstest::rstest]
    #[case(CreateTableMode::Default)]
    #[case(CreateTableMode::IfNotExists)]
    #[case(CreateTableMode::OrReplace)]
    fn preserves_explicit_creation_mode(
        mut test_context: TestParseContext,
        sqlite_syn: SqliteSynBuilder,
        source_span: SourcePathWithSpan,
        #[case] mode: CreateTableMode,
    ) -> testresult::TestResult {
        test_context.set_get_search_path_result(&[
            SchemaName::from("public"),
            SchemaName::from("pg_catalog"),
        ]);
        let Stmt::CreateTable(table) = sqlite_syn.parse_as_stmt("create table users(id integer)")
        else {
            panic!("expected CREATE TABLE");
        };
        let (_, _, _, body) = table.dissolve();
        let CreateTableBody::ColumnsAndConstraint(body) = body else {
            panic!("expected column definitions");
        };
        let (table, _) = CreateTable::analyze_columns_and_constraint(
            test_context,
            &None,
            &sem_builder().table_name("users"),
            &mode,
            true,
            body,
            &Vec::new().into(),
            &source_span,
        )?;
        assert_eq!(table.mode(), &mode);
        assert!(table.temporary());
        Ok(())
    }
}

impl CreateTable {
    pub(crate) fn resolve_ast_foreign_keys(
        ast: &AstTable,
        context: &mut impl ParseContext,
        table: &mut Self,
    ) -> Result<(), AnalysisError> {
        for definition in ast.constraints() {
            if let Some(constraint) =
                Constraint::from_ast_declared_foreign_key(definition, context, table)?
            {
                table.record_declared_constraint(&constraint)?;
            }
        }
        Ok(())
    }
}

impl CreateTable {
    pub(crate) fn resolve_ast_inline_checks(
        ast: &AstTable,
        context: &mut impl ParseContext,
        table: &mut Self,
        names: &mut ConstraintNames,
    ) -> Result<(), AnalysisError> {
        if !ast
            .columns()
            .iter()
            .any(CheckColumnConstraint::has_ast_column_checks)
        {
            return Ok(());
        }
        let target =
            TableName::from(context.creation_name(table.name().full_name(), ast.temporary())?);
        for syntax in ast.columns() {
            if !CheckColumnConstraint::has_ast_column_checks(syntax) {
                continue;
            }
            let name = ColumnName::from(syntax.name().as_str());
            let mut column = table
                .get_column_def(&name)
                .expect("declared column")
                .clone();
            ColumnDefinition::resolve_ast_inline_checks(
                syntax,
                context,
                &target,
                Some(table),
                names,
                &mut column,
            )?;
            table.replace_declared_column(&column);
        }
        Ok(())
    }

    pub(crate) fn resolve_ast_checks(
        ast: &AstTable,
        context: &mut impl ParseContext,
        table: &mut Self,
        names: &mut ConstraintNames,
    ) -> Result<(), AnalysisError> {
        if !ast.constraints().iter().any(AstTableConstraint::is_check) {
            return Ok(());
        }
        let target = context.creation_name(table.name().full_name(), ast.temporary())?;
        for definition in ast.constraints() {
            if definition.is_check() {
                let scope =
                    ScalarAnalysisContext::declared_table(table, &TableName::from(target.clone()));
                let mut check = CheckConstraint::from_ast_expression(
                    definition,
                    context,
                    &TableName::from(target.clone()),
                    &scope,
                    false,
                )?;
                if !names.reserve(check.name()) {
                    check.reject_duplicate_name(context)?;
                }
                table.record_declared_constraint(&Constraint::Check(check))?;
            }
        }
        Ok(())
    }
}

impl DropTable {
    pub(crate) fn from_ast_and_apply<TContext>(
        ast: &AstDropTable,
        mut context: TContext,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError>
    where
        TContext: ParseContext,
    {
        let mut names = Vec::new();
        for name in ast.names() {
            let table_name = TableName::from(FullName::from_ast_name(name)?);
            if context.get_table(&table_name).is_none() {
                context.report_analysis_problem(AnalysisProblem::table_not_found(&table_name))?;
            }
            names.push(table_name);
        }
        let drop_table = Self::new(&names, ast.if_exists(), ast.cascade());
        context = context.apply_drop_table(&drop_table)?;
        Ok(AstAndContextPair::new(
            SemAst::from_drop_table(&drop_table),
            context,
        ))
    }
}

#[cfg(test)]
mod test_analyze_drop_stmt {
    use crate::{
        AstAndContextPair,
        sem::DropTable,
        syn::Opt,
        test_helpers::{SynBuilder, TestParseContext, test_context},
    };

    use testresult::TestResult;

    #[rstest::rstest]
    #[case("drop table table_one", false, false)]
    #[case("drop table if exists table_one", true, false)]
    #[case("drop table table_one cascade", false, true)]
    #[case("drop table table_one restrict", false, false)]
    fn test_analyze_drop_stmt(
        test_context: TestParseContext,
        #[case] sql: &str,
        #[case] if_exists: bool,
        #[case] cascade: bool,
    ) -> TestResult {
        let builder = SynBuilder::new();
        let Some(node) = builder.parse(sql).as_drop_stmt().as_inner() else {
            panic!("expect DropStmt");
        };
        let tokens = Vec::default().into();
        let AstAndContextPair(ast, _result_context) =
            DropTable::analyze_drop_stmt(test_context, node, &tokens)?;
        let Some(drop_table) = ast.as_drop_table() else {
            panic!("{ast:?} is not DropTable");
        };
        assert_eq!(drop_table.if_exists(), if_exists);
        assert_eq!(drop_table.cascade(), cascade);
        let table_name = drop_table.table_names.first().unwrap();
        table_name.assert_table_name("table_one");

        Ok(())
    }
}

#[cfg(all(test, feature = "sqlite"))]
#[cfg(test)]
mod test_analyze_drop_table {
    use crate::{
        AstAndContextPair,
        sem::create_table::DropTable,
        syn::sqlite::DropTableStatement,
        test_helpers::{SqliteSynBuilder, TestParseContext, sqlite_syn, test_context},
    };

    use testresult::TestResult;

    #[rstest::rstest]
    fn test_analyze_drop_table(
        test_context: TestParseContext,
        sqlite_syn: SqliteSynBuilder,
    ) -> TestResult {
        let table_name = sqlite_syn.qualified_name("table_one");
        let statement = DropTableStatement::new(true, table_name);
        let tokens = Vec::default().into();
        let AstAndContextPair(ast, result_context) =
            DropTable::analyze_drop_table(test_context, statement, &tokens)?;
        let Some(drop_table) = ast.as_drop_table() else {
            panic!("{ast:?} is not DropTable")
        };
        assert!(drop_table.if_exists());
        let table_name = drop_table.table_names().first().unwrap();
        table_name.assert_table_name("table_one");
        assert_eq!(result_context.reported_problem_count(), 1);

        Ok(())
    }
}
