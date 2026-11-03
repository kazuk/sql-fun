//! End-to-end fixtures for the expression/query recursive component.

#[cfg(feature = "sqlite")]
mod sqlite_from_groups;
mod stack_regressions;
mod test_helpers;

use self::test_helpers::postgres;

use crate::ast::{StatementNode, TryFromWithTokens};
#[cfg(feature = "sqlite")]
use crate::test_helpers::TestParseContext;
use crate::test_helpers::{SynBuilder, source_span};
use crate::{names::ColumnName, sem::PgBuiltInType};

use ::sql_fun_core::IVec;

#[rstest::rstest]
#[case("select 1")]
#[case("insert into users (id) values (1)")]
#[case("update users set id = 1")]
#[case("delete from users")]
#[case("create extension hstore")]
#[case("set statement_timeout = 0")]
#[case("create type user_kind as enum ('member', 'guest')")]
#[case("create sequence if not exists public.user_id_seq increment by 1")]
#[case("drop table if exists public.users cascade")]
#[case("alter sequence if exists public.user_id_seq restart with 1")]
#[case("comment on table public.users is 'application users'")]
#[case("alter type public.user_kind owner to application_owner")]
#[case("create type public.address as (street text, zip integer)")]
#[case("create table public.users (id integer primary key)")]
#[case("create function public.answer() returns integer language sql as 'select 42'")]
#[case("create schema app create table app.users (id integer)")]
#[case("create domain public.email as text")]
#[case("alter table public.users add column email text")]
#[case("create table public.user_ids as select 1 as id")]
#[case("create index users_id_idx on public.users (id)")]
#[case(
    "create or replace view public.users_view (id) with (security_barrier = true) as select 1 as id"
)]
fn postgres_top_level_statement_construction_is_context_free(
    #[case] sql: &str,
) -> testresult::TestResult {
    let statement = SynBuilder::new().parse(sql);
    StatementNode::try_from_with_tokens(&statement, &IVec::default())?;
    Ok(())
}

#[rstest::rstest]
#[case("create procedure missing(x absent) language sql as 'select 1'")]
#[case("create table missing (id absent, constraint pk primary key (id))")]
#[case("create domain email as absent default 'unknown' check (true)")]
#[case(
    "create schema app create table missing (id absent) create view missing_view as select id from missing"
)]
#[case(
    "create function missing(x absent default unknown()) returns absent language sql as 'select 1'"
)]
#[case("create index missing_idx on missing (unknown(id)) include (extra) where id > 0")]
fn ddl_construction_does_not_require_referenced_objects(
    #[case] sql: &str,
) -> testresult::TestResult {
    let syntax =
        StatementNode::try_from_with_tokens(&SynBuilder::new().parse(sql), &IVec::default())?;
    assert!(matches!(
        syntax,
        StatementNode::Table(_)
            | StatementNode::Domain(_)
            | StatementNode::Schema(_)
            | StatementNode::Function(_)
            | StatementNode::Index(_)
    ));
    Ok(())
}

#[rstest::rstest]
fn table_resolution_uses_common_context(
    source_span: crate::sem::SourcePathWithSpan,
) -> testresult::TestResult {
    let syntax = StatementNode::try_from_with_tokens(
        &SynBuilder::new().parse("create table items (id integer default 1)"),
        &IVec::default(),
    )?;
    let StatementNode::Table(table) = syntax else {
        panic!("table syntax");
    };
    let mut catalog = test_helpers::AnalysisFixture::context();
    let table = table.resolve(&mut catalog, &source_span)?;
    let id = table.get_column_def(&ColumnName::from("id")).expect("id");
    assert_eq!(id.get_type(), Some(&crate::sem::PgBuiltInType::int4()));
    assert!(id.constraints()[0].as_default().is_some());
    assert_eq!(catalog.reported_problem_count(), 0);
    Ok(())
}

#[rstest::rstest]
#[case("create domain label as text")]
#[case(
    "create function answer(x integer default 1) returns integer language sql strict as 'select x'"
)]
#[case("create index items_idx on items (id desc) include (extra)")]
fn ddl_resolution_uses_common_context(#[case] sql: &str) -> testresult::TestResult {
    let syntax =
        StatementNode::try_from_with_tokens(&SynBuilder::new().parse(sql), &IVec::default())?;
    let mut catalog = test_helpers::AnalysisFixture::with_index_table();
    syntax.resolve(
        &mut catalog,
        &mut crate::test_helpers::create_cache_context(),
    )?;
    assert_eq!(catalog.reported_problem_count(), 0);
    Ok(())
}

#[cfg(feature = "sqlite")]
#[rstest::rstest]
#[case("create table items (id integer default 1, label text not null)", true)]
#[case("alter table items add column label text default 'new'", false)]
#[case("alter table items drop column label", false)]
#[case("alter table items rename column label to title", false)]
#[case("alter table items rename to renamed", false)]
fn sqlite_ddl_owns_syntax_after_parser_input_is_dropped(
    #[case] sql: &str,
    #[case] is_create: bool,
) -> testresult::TestResult {
    let syntax = {
        let parsed = crate::test_helpers::SqliteSynBuilder.parse_as_stmt(sql);
        StatementNode::try_from(&parsed)?
    };
    assert_eq!(matches!(syntax, StatementNode::Table(_)), is_create);
    assert_eq!(
        matches!(syntax, StatementNode::AlterRelation(_)),
        !is_create
    );
    Ok(())
}

#[cfg(feature = "sqlite")]
#[test]
fn sqlite_table_retains_named_constraints_order_and_options() -> testresult::TestResult {
    let parsed = crate::test_helpers::SqliteSynBuilder.parse_as_stmt(
        "create table items (id integer, label text, constraint pk primary key (id desc)) without rowid, strict",
    );
    let StatementNode::Table(table) = StatementNode::try_from(&parsed)? else {
        panic!("table syntax");
    };
    assert_eq!(table.constraints[0].name.as_deref(), Some("pk"));
    assert_eq!(table.constraints[0].order[0].descending, Some(true));
    assert_eq!(table.options.len(), 2);
    Ok(())
}

#[cfg(feature = "sqlite")]
#[rstest::rstest]
fn public_sqlite_table_dispatch_preserves_column_semantics(
    source_span: crate::sem::SourcePathWithSpan,
) -> testresult::TestResult {
    use crate::{context::MutSourcePathWithSpan, sem::SemAst};
    let parsed = crate::test_helpers::SqliteSynBuilder
        .parse_as_stmt("create table items (label text not null default 'one')");
    let mut context = TestParseContext::default().with_dialect(crate::ast::Dialect::sqlite());
    context.set_get_search_path_result(&[
        crate::names::SchemaName::from("public"),
        crate::names::SchemaName::from("pg_catalog"),
    ]);
    context.setup_type(crate::sem::PgBuiltInType::text());
    context.set_source_path_with_span(&source_span);
    let crate::AstAndContextPair(statement, context) =
        SemAst::semantic_analysis_sqlite_stmt(context, parsed, &IVec::default())?;
    let SemAst::CreateTable(table) = statement else {
        panic!("semantic table");
    };
    let column = table
        .get_column_def(&ColumnName::from("label"))
        .expect("label");
    assert_eq!(column.is_not_null(), Some(true));
    assert!(column.constraints()[1].as_default().is_some());
    assert_eq!(context.reported_problem_count(), 0);
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
    let mut catalog = test_helpers::AnalysisFixture::context();
    let query = postgres(sql)?.resolve(
        &mut catalog,
        &mut crate::test_helpers::create_cache_context(),
    )?;
    let column = query
        .result_columns()
        .get_result_column_def(&ColumnName::from("n"))
        .expect("n");
    assert_eq!(column.get_type_cloned(), Some(PgBuiltInType::int4()));
    assert_eq!(column.is_not_null(), Some(not_null));
    assert_eq!(catalog.reported_problem_count(), 0);
    Ok(())
}

#[test]
fn correlated_scalar_subquery_uses_outer_scope() -> testresult::TestResult {
    let mut catalog = test_helpers::AnalysisFixture::context();
    let query =
        postgres("select (select outer_query.n) as result from (select 42 as n) outer_query")?
            .resolve(
                &mut catalog,
                &mut crate::test_helpers::create_cache_context(),
            )?;
    assert!(
        query
            .result_columns()
            .get_result_column_def(&ColumnName::from("result"))
            .expect("result")
            .get_type_cloned()
            .is_some()
    );
    assert_eq!(catalog.reported_problem_count(), 0);
    Ok(())
}

#[test]
fn qualified_operator_uses_its_schema_during_lookup() -> testresult::TestResult {
    let mut catalog = test_helpers::AnalysisFixture::with_qualified_addition();
    let query = postgres("select 1 operator(pg_catalog.+) 2 as n")?.resolve(
        &mut catalog,
        &mut crate::test_helpers::create_cache_context(),
    )?;
    let column = query
        .result_columns()
        .get_result_column_def(&ColumnName::from("n"))
        .expect("n");
    assert_eq!(column.get_type_cloned(), Some(PgBuiltInType::int4()));
    // Preserve the existing binary-operator nullability baseline.
    assert_eq!(column.is_not_null(), Some(false));
    assert_eq!(catalog.reported_problem_count(), 0);
    Ok(())
}

#[test]
fn non_lateral_query_does_not_see_sibling_from_item() -> testresult::TestResult {
    let mut catalog = test_helpers::AnalysisFixture::context();
    postgres("select q.n as result from (select 42 as n) a, (select a.n) q")?.resolve(
        &mut catalog,
        &mut crate::test_helpers::create_cache_context(),
    )?;
    assert_eq!(catalog.reported_problem_count(), 1);
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
    use crate::ast::Expr;
    use crate::ast::tests::test_helpers::{AnalysisFixture, postgres, sqlite};
    use crate::test_helpers::{SqliteSynBuilder, TestParseContext, source_span};
    use crate::{
        context::MutSourcePathWithSpan,
        names::ColumnName,
        sem::{PgBuiltInType, SemAst},
    };

    use ::sql_fun_core::IVec;

    #[test]
    fn sqlite_case_does_not_use_postgres_implicit_casts() -> testresult::TestResult {
        let sql = "select case when true then 1 else '2' end as n";
        let mut catalog = AnalysisFixture::with_integer_to_text_cast();
        let pg = postgres(sql)?.resolve(
            &mut catalog,
            &mut crate::test_helpers::create_cache_context(),
        )?;
        catalog = catalog.with_dialect(crate::ast::Dialect::sqlite());
        let sqlite = sqlite(sql)?.resolve(
            &mut catalog,
            &mut crate::test_helpers::create_cache_context(),
        )?;
        let name = ColumnName::from("n");
        assert_eq!(
            pg.result_columns()
                .get_result_column_def(&name)
                .expect("n")
                .get_type_cloned(),
            Some(PgBuiltInType::int4())
        );
        assert_eq!(
            sqlite
                .result_columns()
                .get_result_column_def(&name)
                .expect("n")
                .get_type_cloned(),
            None
        );
        assert_eq!(catalog.reported_problem_count(), 0);
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
        let mut catalog = AnalysisFixture::context();
        let pg = postgres(sql)?.resolve(
            &mut catalog,
            &mut crate::test_helpers::create_cache_context(),
        )?;
        catalog = catalog.with_dialect(crate::ast::Dialect::sqlite());
        let sqlite = sqlite(sql)?.resolve(
            &mut catalog,
            &mut crate::test_helpers::create_cache_context(),
        )?;
        assert_eq!(pg, sqlite);
        assert_eq!(catalog.reported_problem_count(), 0);
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
        let mut context = TestParseContext::default().with_dialect(crate::ast::Dialect::sqlite());
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
        let mut context = TestParseContext::default().with_dialect(crate::ast::Dialect::sqlite());
        let result = syntax.resolve(&mut context, &crate::sem::ScalarAnalysisContext::default())?;
        assert_eq!(result.get_type(), Some(PgBuiltInType::int4()));
        assert_eq!(context.reported_problem_count(), 0);
        Ok(())
    }
}

#[cfg(feature = "sqlite")]
#[rstest::rstest]
#[case::namespace("$ns::name")]
#[case::suffix("$name(element)")]
#[case::namespace_and_suffix("$ns::name(element)")]
#[case::colon(":name")]
#[case::at("@name")]
fn sqlite_upgrade_preserves_complete_parameter_names(#[case] name: &str) -> testresult::TestResult {
    use crate::ast::Expr;
    use crate::test_helpers::SqliteSynBuilder;

    let parsed = SqliteSynBuilder.parse_expression(&format!("SELECT {name}"));
    let Expr::Parameter(parameter) = Expr::try_from(&parsed)? else {
        panic!("parameter");
    };
    assert_eq!(parameter.name.as_deref(), Some(name));
    assert_eq!(parameter.position, 1);
    assert!(parameter.type_name.is_none());
    Ok(())
}

#[cfg(feature = "sqlite")]
#[test]
fn sqlite_upgrade_preserves_parameter_reuse() -> testresult::TestResult {
    use crate::ast::Expr;
    use crate::test_helpers::SqliteSynBuilder;

    let parsed =
        SqliteSynBuilder.parse_expression("SELECT $ns::name(element) + $ns::name(element)");
    let Expr::Operator(operator) = Expr::try_from(&parsed)? else {
        panic!("addition");
    };
    let Expr::Parameter(left) = operator.left.as_ref().unwrap() else {
        panic!("left parameter");
    };
    let Expr::Parameter(right) = operator.right.as_ref().unwrap() else {
        panic!("right parameter");
    };
    assert_eq!(left.name.as_deref(), Some("$ns::name(element)"));
    assert_eq!(right.name, left.name);
    assert_eq!(left.position, 1);
    assert_eq!(right.position, left.position);
    Ok(())
}

#[cfg(feature = "sqlite")]
#[rstest::rstest]
fn sqlite_parenthesized_in_subqueries_keep_membership_semantics(
    #[values("(SELECT 2)", "((SELECT 2))", "(((SELECT 2)))")] rhs: &str,
) -> testresult::TestResult {
    use crate::ast::{BooleanOperator, Expr, Literal, SubqueryKind};
    use crate::test_helpers::SqliteSynBuilder;

    let parsed = SqliteSynBuilder.parse_expression(&format!("SELECT 1 IN {rhs}"));
    let Expr::Subquery(query) = Expr::try_from(&parsed)? else {
        panic!("membership subquery");
    };
    assert!(matches!(query.kind, SubqueryKind::Any));
    assert!(matches!(
        query.test,
        Some(Expr::Literal(Literal::Integer(1)))
    ));
    assert_eq!(query.operator.full_name()?.name().as_str(), "=");
    let parsed = SqliteSynBuilder.parse_expression(&format!("SELECT 1 NOT IN {rhs}"));
    let Expr::Boolean(negation) = Expr::try_from(&parsed)? else {
        panic!("negated membership");
    };
    assert!(matches!(negation.operator, BooleanOperator::Not));
    assert_eq!(negation.arguments.len(), 1);
    let Expr::Subquery(query) = &negation.arguments[0] else {
        panic!("membership subquery");
    };
    assert!(matches!(query.kind, SubqueryKind::Any));
    assert!(matches!(
        query.test,
        Some(Expr::Literal(Literal::Integer(1)))
    ));
    Ok(())
}

#[cfg(feature = "sqlite")]
#[test]
fn sqlite_scalar_subquery_keeps_scalar_semantics() -> testresult::TestResult {
    use crate::ast::{Expr, SubqueryKind};
    use crate::test_helpers::SqliteSynBuilder;

    let parsed = SqliteSynBuilder.parse_expression("SELECT (SELECT 2)");
    let Expr::Subquery(query) = Expr::try_from(&parsed)? else {
        panic!("scalar subquery");
    };
    assert!(matches!(query.kind, SubqueryKind::Scalar));
    assert!(query.test.is_none());
    Ok(())
}

#[cfg(feature = "sqlite")]
#[rstest::rstest]
#[case::lowercase("SELECT x'ab'", "ab")]
#[case::uppercase("SELECT X'AB'", "AB")]
#[case::mixed_case("SELECT x'aBcD'", "aBcD")]
#[case::empty("SELECT x''", "")]
fn sqlite_blob_semantics_keep_hex_payload_after_spelling_upgrade(
    #[case] sql: &str,
    #[case] payload: &str,
) -> testresult::TestResult {
    use crate::ast::{Expr, Literal};
    use crate::test_helpers::SqliteSynBuilder;

    let parsed = SqliteSynBuilder.parse_expression(sql);
    let Expr::Literal(Literal::Blob(value)) = Expr::try_from(&parsed)? else {
        panic!("blob literal");
    };
    assert_eq!(value, payload);
    Ok(())
}

#[test]
fn missing_index_table_reports_resolution_problem() -> testresult::TestResult {
    let syntax = StatementNode::try_from_with_tokens(
        &SynBuilder::new().parse("CREATE INDEX idx ON missing.items(id)"),
        &IVec::default(),
    )?;
    let mut catalog = test_helpers::AnalysisFixture::context();
    syntax.resolve(
        &mut catalog,
        &mut crate::test_helpers::create_cache_context(),
    )?;
    assert_eq!(catalog.reported_problem_count(), 1);
    Ok(())
}
