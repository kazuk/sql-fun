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

#[cfg(test)]
mod test_create_table_foreign_keys {
    use crate::{
        context::TableInfoRead,
        names::ColumnName,
        test_helpers::{foreign_keys as fk, ownership::apply_ownership_sql},
    };

    #[test]
    fn create_result_and_catalog_retain_composite_and_unnamed_foreign_keys()
    -> testresult::TestResult {
        let context = fk::apply(
            fk::context(),
            "CREATE TABLE a.parent(x integer, y integer, PRIMARY KEY(x,y), UNIQUE(y,x))",
        );
        let pair = apply_ownership_sql(
            context,
            "CREATE TABLE a.child(x integer, y integer, CONSTRAINT fk FOREIGN KEY(y,x) REFERENCES parent(y,x), FOREIGN KEY(x,y) REFERENCES parent, FOREIGN KEY(y,x) REFERENCES parent(y,x))",
        )?;
        let result = pair.ast().as_create_table().unwrap();
        assert_eq!(result.foreign_keys().count(), 3);
        let constraint = result.constraint("fk").unwrap().as_foreign_key().unwrap();
        assert_eq!(
            constraint.source_columns(),
            &[ColumnName::from("y"), ColumnName::from("x")]
        );
        assert_eq!(
            constraint.target_columns(),
            Some([ColumnName::from("y"), ColumnName::from("x")].as_slice())
        );
        let context = pair.into_context();
        let child = context
            .get_table_impl(fk::name("a", "child").full_name())
            .unwrap();
        assert_eq!(child.foreign_keys().count(), 3);
        assert_eq!(
            child
                .foreign_keys()
                .filter(|key| key.name().is_empty())
                .count(),
            2
        );
        assert!(child.foreign_keys().any(|key| key.target_columns()
            == Some([ColumnName::from("x"), ColumnName::from("y")].as_slice())));
        assert!(context.problems().is_empty(), "{:?}", context.problems());
        Ok(())
    }

    #[rstest::rstest]
    #[case::source("absent", "parent(id)", "column_not_found_in_table", true)]
    #[case::postgres_case_sensitive("\"ID\"", "parent(id)", "column_not_found_in_table", true)]
    #[case::target_column("id", "parent(absent)", "column_not_found_in_table", true)]
    #[case::target_relation("id", "missing(id)", "relation_not_found", false)]
    fn rejected_create_retains_analysis_without_publishing_table(
        #[case] source: &str,
        #[case] reference: &str,
        #[case] code: &str,
        #[case] bound: bool,
    ) -> testresult::TestResult {
        let context = fk::parents();
        let pair = apply_ownership_sql(
            context,
            &format!(
                "CREATE TABLE a.child(id integer, CONSTRAINT fk FOREIGN KEY({source}) REFERENCES {reference})"
            ),
        )?;
        let table = pair.ast().as_create_table().unwrap();
        let constraint = table.constraint("fk").unwrap().as_foreign_key().unwrap();
        assert!(!constraint.references_can_apply());
        assert_eq!(
            constraint.target_table(),
            if bound {
                Some(fk::name("a", "parent"))
            } else {
                None
            }
            .as_ref()
        );
        let context = pair.into_context();
        assert!(
            context
                .get_table_impl(fk::name("a", "child").full_name())
                .is_none()
        );
        assert_eq!(context.problems().len(), 1);
        assert_eq!(context.problems()[0].code(), code);
        let context = fk::apply(context, "SELECT id FROM a.parent");
        assert_eq!(context.problems().len(), 1);
        Ok(())
    }

    #[cfg(feature = "sqlite")]
    #[rstest::rstest]
    #[case::existing("Parent(id)", true)]
    #[case::forward("later(id)", false)]
    fn sqlite_create_table_foreign_keys_retain_declarations(
        #[case] reference: &str,
        #[case] resolved: bool,
    ) -> testresult::TestResult {
        let context = fk::apply_sqlite(fk::sqlite_context(), "CREATE TABLE Parent(id PRIMARY KEY)");
        let sql = format!(
            "CREATE TABLE child(id, CONSTRAINT fk FOREIGN KEY(id) REFERENCES {reference} ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED)"
        );
        let context = fk::apply_sqlite(context, &sql);
        let child = context
            .get_table_impl(fk::name("main", "child").full_name())
            .unwrap();
        let constraint = child.constraint("fk").unwrap().as_foreign_key().unwrap();
        assert_eq!(
            constraint.target_table(),
            if resolved {
                Some(fk::name("main", "parent"))
            } else {
                None
            }
            .as_ref()
        );
        assert_eq!(
            ::serde_json::to_value(constraint.delete_action())?,
            "Cascade"
        );
        assert!(context.problems().is_empty(), "{:?}", context.problems());
        let oracle = ::rusqlite::Connection::open_in_memory()?;
        oracle.execute_batch("CREATE TABLE Parent(id PRIMARY KEY)")?;
        oracle.execute_batch(&sql)?;
        let target: String = oracle.query_row(
            r#"SELECT "table" FROM pragma_foreign_key_list('child')"#,
            [],
            |row| row.get(0),
        )?;
        assert_eq!(target, if resolved { "Parent" } else { "later" });
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
