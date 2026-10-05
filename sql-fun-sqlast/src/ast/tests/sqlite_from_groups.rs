use crate::sem::{BaseParseContext, ColumnName, PgBuiltInType};

#[rstest::rstest]
#[case("select q.ax as n from (a) as q", false, true)]
#[case("select q.ax as n from ((a)) as q", false, true)]
#[case("select r.ax as n from ((a) as q) as r", false, true)]
#[case("select q.ax as n from (a join b) as q", false, true)]
#[case("select q.bx as n from (a join b) as q", true, true)]
#[case("select a.ax as n from (a join b) as q", false, true)]
#[case("select b.bx as n from (a join b) as q", true, true)]
#[case("select q.bx as n from (a, b) as q", true, true)]
#[case("select q.cx as n from (a join b join c) as q", true, true)]
#[case("select q.bx as n from ((a join b)) as q", true, true)]
#[case("select q.bx as n from (a left join b on true) as q", true, false)]
#[case("select q.ax as n from (a left join b on true) as q", false, true)]
#[case("select q.ax as n from (a right join b on true) as q", false, false)]
#[case("select q.bx as n from (a full join b on true) as q", true, false)]
#[case("select q.id as n from (a join b using(id)) as q", false, true)]
#[case("select q.id as n from (a right join b using(id)) as q", true, true)]
#[case("select q.id as n from (a natural right join b) as q", true, true)]
#[case("select q.id as n from (a left join b using(id)) as q", false, true)]
#[case("select q.id as n from (a natural left join b) as q", false, true)]
#[case("select q.ax as n from (a right join b using(id)) as q", false, false)]
#[case("select q.ax as n from (a natural right join b) as q", false, false)]
#[case("select q.id as n from (a right join b on true) as q", false, false)]
#[case(
    "select q.id as n from ((a join c using(id)) right join b using(id)) as q",
    true,
    true
)]
#[case("select q.id as n from (a join b) as q", false, true)]
#[case("select a.ax as n from (a)", false, true)]
#[case("select b.bx as n from (a join b)", true, true)]
#[case("select ax as n from (a) as q", false, true)]
#[case("select bx as n from (a join b) as q", true, true)]
fn parenthesized_sources_preserve_column_semantics(
    #[case] sql: &str,
    #[case] text: bool,
    #[case] not_null: bool,
) -> testresult::TestResult {
    let (query, context) = test_helpers::analyze(sql)?;
    let column = query
        .get_result_column_def(&ColumnName::from("n"))
        .expect("result");
    let expected_type = if text {
        PgBuiltInType::text()
    } else {
        PgBuiltInType::int4()
    };
    assert_eq!(column.get_type_cloned(), Some(expected_type));
    assert_eq!(column.is_not_null(), Some(not_null));
    assert_eq!(context.reported_problem_count(), 0);
    assert_eq!(context.metadata().name(), "analytics");
    assert_eq!(
        context.metadata().sql_dialect(),
        ::sql_fun_core::SqlDialect::SQLite
    );
    assert_eq!(
        context
            .metadata()
            .engine_version()
            .as_ref()
            .expect("version")
            .to_string(),
        "3"
    );
    Ok(())
}

#[rstest::rstest]
#[case("select a.ax as n from (a) as q")]
#[case("select q.ax as n from ((a) as q) as r")]
#[case("select q.absent as n from (a) as q")]
#[case("select q.absent as n from (a join b) as q")]
#[case("select id as n from (a join b) as q")]
fn parenthesized_sources_report_invalid_references(#[case] sql: &str) -> testresult::TestResult {
    let (query, context) = test_helpers::analyze(sql)?;
    assert_eq!(context.reported_problem_count(), 1);
    assert_eq!(
        query
            .get_result_column_def(&ColumnName::from("n"))
            .expect("result")
            .get_type_cloned(),
        None
    );
    Ok(())
}

#[test]
fn parenthesized_sources_report_duplicate_group_aliases() -> testresult::TestResult {
    let (_, context) = test_helpers::analyze("select 1 from (a) as q, (b) as q")?;
    assert_eq!(context.reported_problem_count(), 1);
    Ok(())
}

mod test_helpers {
    use crate::sem::{
        AnalysisError, BaseParseContext, ColumnDefinition, ColumnName, MutSourcePathWithSpan,
        PgBuiltInType, SchemaName, SelectStatement, SemAst,
    };
    use crate::test_helpers::{SqliteSynBuilder, TestParseContext, source_span};

    fn selected_context() -> TestParseContext {
        let mut metadata = ::sql_fun_core::SqlFunMetadataCollection::load_from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/selected-sqlite.toml"
        ))
        .expect("metadata fixture");
        let selected = metadata.remove("analytics").expect("selected datasource");
        let mut context = TestParseContext::default().with_metadata(&selected);
        context.set_source_path_with_span(&source_span());
        context.set_get_search_path_result(&[SchemaName::from("public")]);
        context.setup_type(PgBuiltInType::int4());
        context.setup_type(PgBuiltInType::text());
        context.setup_type(PgBuiltInType::bool());
        for (table, column, ty) in [
            ("a", "ax", PgBuiltInType::int4()),
            ("b", "bx", PgBuiltInType::text()),
            ("c", "cx", PgBuiltInType::text()),
        ] {
            context.setup_table(
                table,
                &[
                    ColumnDefinition::new(
                        &Some(ColumnName::from(column)),
                        Some(&ty),
                        Some(true),
                        &[],
                    ),
                    ColumnDefinition::new(
                        &Some(ColumnName::from("id")),
                        Some(&ty),
                        Some(true),
                        &[],
                    ),
                ],
                &source_span(),
            );
        }
        context
    }

    pub(super) fn analyze(sql: &str) -> Result<(SelectStatement, TestParseContext), AnalysisError> {
        let context = selected_context();
        let statement = SqliteSynBuilder.parse_as_stmt(sql);
        let crate::AstAndContextPair(ast, context) = SemAst::semantic_analysis_sqlite_stmt(
            context,
            statement,
            &::sql_fun_core::IVec::default(),
        )?;
        assert_eq!(
            context.metadata().connector().as_deref(),
            Some("selected-connector")
        );
        Ok((ast.into_select_statemtnt().expect("SELECT"), context))
    }
}
