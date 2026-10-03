//! End-to-end fixtures for the expression/query recursive component.

mod test_helpers;

use self::test_helpers::postgres;

use crate::sem::unresolved::{Dialect, Statement, TryFromWithTokens};
use crate::sem::{ColumnName, PgBuiltInType};
use crate::test_helpers::{SynBuilder, TestParseContext};

use ::sql_fun_core::IVec;

#[rstest::rstest]
#[case("select 1")]
#[case("insert into users (id) values (1)")]
#[case("update users set id = 1")]
#[case("delete from users")]
#[case("create extension hstore")]
#[case("set statement_timeout = 0")]
#[case(
    "create or replace view public.users_view (id) with (security_barrier = true) as select 1 as id"
)]
fn postgres_top_level_statement_construction_is_context_free(
    #[case] sql: &str,
) -> testresult::TestResult {
    let statement = SynBuilder::new().parse(sql);
    Statement::try_from_with_tokens(&statement, &IVec::default())?;
    Ok(())
}

#[rstest::rstest]
#[case(
    "select unknown_schema.unknown_function(missing) from absent",
    "unknown_function"
)]
#[case(
    "select distinct on (a) a from missing group by distinct rollup(a), cube(a), grouping sets ((), (a)) having a is not null",
    "Rollup"
)]
#[case(
    "select sum(a order by b) filter (where c) over (partition by d order by e rows between 2 preceding and 1 following exclude ties) from missing",
    "Ties"
)]
#[case(
    "select f(variadic a), f(x => a), row(a,b), (array[1,2])[1] from missing",
    "NamedArgument"
)]
#[case(
    "select (select a from missing) union all (select 2 intersect select 3) order by 1 limit 4 offset 2",
    "Intersect"
)]
#[case(
    "select a from missing tablesample system(10) repeatable(4) for update of missing skip locked",
    "Sample"
)]
#[case(
    "select 1 from lateral (select absent) q(x) left join other o using(x) as j",
    "lateral: true"
)]
#[case(
    "select 1 from rows from(f(1) as (x int), g()) with ordinality as r",
    "rows_from: true"
)]
#[case(
    "with recursive c(x) as not materialized (select 1 union all select x from c) search depth first by x set seq cycle x set mark to true default false using path select x from c",
    "Cycle"
)]
#[case(
    "with c as (insert into missing(x) overriding system value select 1 on conflict (x) where x > 0 do update set x = excluded.x returning x) select 1",
    "Conflict"
)]
#[case(
    "with c as (update missing set (x,y) = (select 1,2) from other where true returning x) select 1",
    "MultiAssignment"
)]
#[case(
    "with c as (delete from missing using other where true returning x) select 1",
    "Delete"
)]
#[case(
    "with c as (merge into missing m using other o on true when matched then update set x = 1 when not matched then insert(x) values(2) returning m.x) select 1",
    "Merge"
)]
#[case("select x into temporary t from missing", "into: Some")]
#[case(
    "with c as (insert into missing values (1) on conflict (x text_ops(foo = text)) do nothing) select 1",
    "SyntaxOption"
)]
#[case(
    "select cast($1 as numeric(10,2)[]), a collate \"C\", a is not true from missing",
    "TruthTest"
)]
#[case("select grouping(a) from missing group by rollup(a)", "Grouping")]
#[case(
    "select xmlconcat('<a/>'), xmlelement(name a, xmlattributes(x as b), y), xmlforest(x as z) from missing",
    "named_args"
)]
#[case(
    "select xmlparse(document '<a/>' preserve whitespace), xmlserialize(content '<a/>' as text indent)",
    "XmlSerialize"
)]
#[case(
    "select x from xmltable(xmlnamespaces('urn:x' as ns), '/x' passing '<x/>' columns n for ordinality, x text path '.' default 'fallback' not null) as t",
    "XmlColumn"
)]
#[case(
    "select json_object('k': x absent on null with unique keys returning jsonb), json_array(1,2 null on null) from missing",
    "JsonObject"
)]
#[case(
    "select json_array(select a from missing returning jsonb)",
    "JsonArrayQuery"
)]
#[case(
    "select json('{ \"a\": 1 }' with unique keys), json_scalar(1), json_serialize('{}' returning text)",
    "JsonSerialize"
)]
#[case(
    "select json_objectagg(k: v absent on null) filter(where true) over(), json_arrayagg(v order by k) from missing",
    "JsonAggregate"
)]
#[case(
    "select x is json object with unique keys from missing",
    "JsonPredicate"
)]
#[case(
    "select json_query('{}', '$' passing x as a returning jsonb with conditional wrapper omit quotes on scalar string empty array on empty error on error) from missing",
    "JsonFunction"
)]
#[case(
    "select x from json_table('{}', '$' as root passing 1 as arg columns(n for ordinality, x text path '$.x' default 'fallback' on empty, nested path '$.a[*]' columns(y int path '$')) error on error) as j",
    "JsonColumn"
)]
fn context_free_postgres_preserves_recursive_syntax(
    #[case] sql: &str,
    #[case] marker: &str,
) -> testresult::TestResult {
    let syntax = postgres(sql)?;
    let snapshot = format!("{syntax:#?}");
    assert!(snapshot.contains(marker), "{snapshot}");
    Ok(())
}

#[rstest::rstest]
#[case("select 42 as n", true)]
#[case(
    "select q.n as n from (select 42 as n) a, lateral (select a.n) q",
    true
)]
#[case("select q.n from (select 42 as n) q", true)]
#[case("with c(x) as (select 42) select c.x as n from c", true)]
#[case(
    "with c(x) as (select 42) select q.x as n from (select x from c) q",
    true
)]
#[case(
    "select case when true then q.n else 42 end as n from (select 1 as n) q",
    true
)]
#[case(
    "select b.n from (select 1 as n) a left join (select 2 as n) b on true",
    false
)]
#[case("select 1 as n union all select 2 union select 3", true)]
#[case("(select 1 as n intersect select 2) union all select 3", true)]
fn shared_resolution_preserves_result_metadata(
    #[case] sql: &str,
    #[case] not_null: bool,
) -> testresult::TestResult {
    let catalog = test_helpers::ReadOnlyCatalog::new();
    let mut diagnostics = TestParseContext::default();
    let query = postgres(sql)?
        .resolve(&catalog, Dialect::postgres())
        .report(&mut diagnostics)?;
    let column = query
        .result_columns()
        .get_result_column_def(&ColumnName::from("n"))
        .expect("n");
    assert_eq!(column.get_type_cloned(), Some(PgBuiltInType::int4()));
    assert_eq!(column.is_not_null(), Some(not_null));
    assert_eq!(diagnostics.reported_problem_count(), 0);
    Ok(())
}

#[test]
fn correlated_scalar_subquery_uses_outer_scope() -> testresult::TestResult {
    let catalog = test_helpers::ReadOnlyCatalog::new();
    let mut diagnostics = TestParseContext::default();
    let query =
        postgres("select (select outer_query.n) as result from (select 42 as n) outer_query")?
            .resolve(&catalog, Dialect::postgres())
            .report(&mut diagnostics)?;
    assert!(
        query
            .result_columns()
            .get_result_column_def(&ColumnName::from("result"))
            .expect("result")
            .get_type_cloned()
            .is_some()
    );
    assert_eq!(diagnostics.reported_problem_count(), 0);
    Ok(())
}

#[test]
fn qualified_operator_uses_its_schema_during_lookup() -> testresult::TestResult {
    let catalog = test_helpers::ReadOnlyCatalog::with_qualified_addition();
    let mut diagnostics = TestParseContext::default();
    let query = postgres("select 1 operator(pg_catalog.+) 2 as n")?
        .resolve(&catalog, Dialect::postgres())
        .report(&mut diagnostics)?;
    let column = query
        .result_columns()
        .get_result_column_def(&ColumnName::from("n"))
        .expect("n");
    assert_eq!(column.get_type_cloned(), Some(PgBuiltInType::int4()));
    // Preserve the existing binary-operator nullability baseline.
    assert_eq!(column.is_not_null(), Some(false));
    assert_eq!(diagnostics.reported_problem_count(), 0);
    Ok(())
}

#[test]
fn non_lateral_query_does_not_see_sibling_from_item() -> testresult::TestResult {
    let catalog = test_helpers::ReadOnlyCatalog::new();
    let mut diagnostics = TestParseContext::default();
    postgres("select q.n as result from (select 42 as n) a, (select a.n) q")?
        .resolve(&catalog, Dialect::postgres())
        .report(&mut diagnostics)?;
    assert_eq!(diagnostics.reported_problem_count(), 1);
    Ok(())
}

#[rstest::rstest]
#[case("with c as (delete from missing where current of cursor_name returning x) select 1")]
#[case(
    "with c as (merge into missing using other on true when matched then do nothing returning merge_action()) select 1"
)]
fn context_free_dml_expression_syntax(#[case] sql: &str) -> testresult::TestResult {
    postgres(sql)?;
    Ok(())
}

#[cfg(feature = "sqlite")]
mod sqlite {
    use crate::sem::unresolved::tests::test_helpers::{ReadOnlyCatalog, postgres, sqlite};
    use crate::sem::unresolved::{Dialect, Expr};
    use crate::sem::{ColumnName, MutSourcePathWithSpan, PgBuiltInType, SemAst};
    use crate::test_helpers::{SqliteSynBuilder, TestParseContext, source_span};

    use ::sql_fun_core::IVec;

    #[test]
    fn sqlite_case_does_not_use_postgres_implicit_casts() -> testresult::TestResult {
        let sql = "select case when true then 1 else 'text' end as n";
        let catalog = ReadOnlyCatalog::with_integer_to_text_cast();
        let mut diagnostics = TestParseContext::default();
        let pg = postgres(sql)?
            .resolve(&catalog, Dialect::postgres())
            .report(&mut diagnostics)?;
        let sqlite = sqlite(sql)?
            .resolve(&catalog, Dialect::sqlite())
            .report(&mut diagnostics)?;
        let name = ColumnName::from("n");
        assert_eq!(
            pg.result_columns()
                .get_result_column_def(&name)
                .expect("n")
                .get_type_cloned(),
            Some(PgBuiltInType::text())
        );
        assert_eq!(
            sqlite
                .result_columns()
                .get_result_column_def(&name)
                .expect("n")
                .get_type_cloned(),
            None
        );
        assert_eq!(diagnostics.reported_problem_count(), 0);
        Ok(())
    }

    #[rstest::rstest]
    #[case("select 42 as n")]
    #[case("select q.n as n from (select 42 as n) q")]
    #[case("with c(x) as (select 42) select c.x as n from c")]
    #[case("select case when true then q.n else 42 end as n from (select 1 as n) q")]
    #[case("select b.n as n from (select 1 as n) a left join (select 2 as n) b on true")]
    #[case("select 1 as n union all select 2 as n union select 3 as n")]
    fn both_frontends_use_the_same_resolver(#[case] sql: &str) -> testresult::TestResult {
        let catalog = ReadOnlyCatalog::new();
        let mut diagnostics = TestParseContext::default();
        let pg = postgres(sql)?
            .resolve(&catalog, Dialect::postgres())
            .report(&mut diagnostics)?;
        let sqlite = sqlite(sql)?
            .resolve(&catalog, Dialect::sqlite())
            .report(&mut diagnostics)?;
        assert_eq!(pg, sqlite);
        assert_eq!(diagnostics.reported_problem_count(), 0);
        Ok(())
    }

    #[rstest::rstest]
    #[case(
        "select missing, $named, ?2, x'abc0', 'a''b' from absent indexed by idx",
        "not_indexed: false"
    )]
    #[case(
        "with recursive c(x) as not materialized (select 1 union all select x from c) select distinct x from c order by x desc nulls first limit 4 offset 2",
        "recursive: true"
    )]
    #[case(
        "select f(a order by b) filter(where c) over (partition by d order by e groups between 1 preceding and current row exclude no others) from missing",
        "NoOthers"
    )]
    #[case(
        "select a in (select x from missing), exists(select 1), a in other(1), a not like 'x' escape '!' from missing",
        "InTable"
    )]
    #[case("select 1 from (missing left join other using(x)) as q", "FromGroup")]
    fn context_free_sqlite_preserves_syntax(
        #[case] sql: &str,
        #[case] marker: &str,
    ) -> testresult::TestResult {
        let common = sqlite(sql)?;
        let snapshot = format!("{common:#?}");
        assert!(snapshot.contains(marker), "{snapshot}");
        Ok(())
    }

    #[test]
    fn public_sqlite_select_dispatch_resolves_cte_columns() -> testresult::TestResult {
        let mut context = TestParseContext::default();
        context.set_source_path_with_span(&source_span());
        let statement =
            SqliteSynBuilder.parse_as_stmt("with c(x) as (select 42) select x as n from c");
        let crate::AstAndContextPair(ast, context) =
            SemAst::semantic_analysis_sqlite_stmt(context, statement, &IVec::default())?;
        let query = ast.into_select_statemtnt().expect("SELECT result");
        let column = query
            .result_columns()
            .get_result_column_def(&ColumnName::from("n"))
            .expect("projection");
        assert_eq!(column.get_type_cloned(), Some(PgBuiltInType::int4()));
        assert_eq!(context.reported_problem_count(), 0);
        Ok(())
    }

    #[test]
    fn sqlite_unary_literal_does_not_use_postgres_operator_catalog() -> testresult::TestResult {
        let syntax = Expr::try_from(&SqliteSynBuilder.parse_expression("select -42"))?;
        let mut context = TestParseContext::default();
        let result = syntax
            .resolve(
                &context,
                Dialect::sqlite(),
                &crate::sem::ScalarAnalysisContext::default(),
            )
            .report(&mut context)?;
        assert_eq!(result.get_type(), Some(PgBuiltInType::int4()));
        assert_eq!(context.reported_problem_count(), 0);
        Ok(())
    }
}
