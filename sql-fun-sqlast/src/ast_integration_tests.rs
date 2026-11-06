//! End-to-end fixtures for the expression/query recursive component.

#[cfg(feature = "sqlite")]
use crate::{
    AstAndContextPair,
    ast::Dialect,
    names::{FullName, SchemaName},
    parse_statements,
    test_helpers::{SqliteSynBuilder as TestHelpersSqliteSynBuilder, TestParseContext},
};
use crate::{
    ast::{StatementNode, TryFromWithTokens},
    names::ColumnName,
    sem::{CreateTable, PgBuiltInType, SelectStatement, SemAst, SourcePathWithSpan},
    test_helpers::{
        SynBuilder, ast_query as test_helpers, ast_query::postgres, create_cache_context,
        source_span,
    },
};

#[cfg(feature = "sqlite")]
mod sqlite_from_groups;
mod stack_regressions;

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
fn table_resolution_uses_common_context(source_span: SourcePathWithSpan) -> testresult::TestResult {
    let syntax = StatementNode::try_from_with_tokens(
        &SynBuilder::new().parse("create table items (id integer default 1)"),
        &IVec::default(),
    )?;
    let StatementNode::Table(table) = syntax else {
        panic!("table syntax");
    };
    let mut catalog = test_helpers::AnalysisFixture::context();
    let table = CreateTable::from_ast(&table, &mut catalog, &source_span)?;
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
    SemAst::from_ast_node(&syntax, &mut catalog, &mut create_cache_context())?;
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
        let parsed = TestHelpersSqliteSynBuilder.parse_as_stmt(sql);
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
    let parsed = TestHelpersSqliteSynBuilder.parse_as_stmt(
        "create table items (id integer, label text, constraint pk primary key (id desc)) without rowid, strict",
    );
    let StatementNode::Table(table) = StatementNode::try_from(&parsed)? else {
        panic!("table syntax");
    };
    assert_eq!(table.constraints()[0].name().as_deref(), Some("pk"));
    assert_eq!(table.constraints()[0].order()[0].descending(), Some(true));
    assert_eq!(table.options().len(), 2);
    Ok(())
}

#[cfg(feature = "sqlite")]
#[rstest::rstest]
fn public_sqlite_table_dispatch_preserves_column_semantics(
    source_span: SourcePathWithSpan,
) -> testresult::TestResult {
    use crate::{context::MutSourcePathWithSpan, sem::SemAst};
    let parsed = parse_statements(
        "create table items (label text not null default 'one')",
        Dialect::sqlite(),
    )
    .next()
    .unwrap()?;
    let mut context = TestParseContext::default().with_dialect(Dialect::sqlite());
    context
        .set_get_search_path_result(&[SchemaName::from("public"), SchemaName::from("pg_catalog")]);
    context.setup_type(PgBuiltInType::text());
    context.set_source_path_with_span(&source_span);
    let AstAndContextPair(statement, context) =
        SemAst::from_ast(&parsed, context, &mut create_cache_context())?;
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
    let query =
        SelectStatement::from_ast(&postgres(sql)?, &mut catalog, &mut create_cache_context())?;
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
    let query = SelectStatement::from_ast(
        &postgres("select (select outer_query.n) as result from (select 42 as n) outer_query")?,
        &mut catalog,
        &mut create_cache_context(),
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
    let query = SelectStatement::from_ast(
        &postgres("select 1 operator(pg_catalog.+) 2 as n")?,
        &mut catalog,
        &mut create_cache_context(),
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
    SelectStatement::from_ast(
        &postgres("select q.n as result from (select 42 as n) a, (select a.n) q")?,
        &mut catalog,
        &mut create_cache_context(),
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
    use crate::{
        AstAndContextPair,
        ast::{Dialect, Expr},
        context::MutSourcePathWithSpan,
        names::ColumnName,
        parse_statements,
        sem::{PgBuiltInType, ScalarAnalysisContext, SelectStatement, SemAst, SemScalarExpr},
        test_helpers::{
            ast_query::{postgres, sqlite},
            create_cache_context,
        },
    };

    #[cfg(feature = "sqlite")]
    use crate::test_helpers::{
        SqliteSynBuilder, TestParseContext, ast_query::AnalysisFixture, source_span,
    };

    #[test]
    fn sqlite_case_does_not_use_postgres_implicit_casts() -> testresult::TestResult {
        let sql = "select case when true then 1 else '2' end as n";
        let mut catalog = AnalysisFixture::with_integer_to_text_cast();
        let pg =
            SelectStatement::from_ast(&postgres(sql)?, &mut catalog, &mut create_cache_context())?;
        catalog = catalog.with_dialect(Dialect::sqlite());
        let sqlite =
            SelectStatement::from_ast(&sqlite(sql)?, &mut catalog, &mut create_cache_context())?;
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
        let pg =
            SelectStatement::from_ast(&postgres(sql)?, &mut catalog, &mut create_cache_context())?;
        catalog = catalog.with_dialect(Dialect::sqlite());
        let sqlite =
            SelectStatement::from_ast(&sqlite(sql)?, &mut catalog, &mut create_cache_context())?;
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
        let mut context = TestParseContext::default().with_dialect(Dialect::sqlite());
        context.set_source_path_with_span(&source_span());
        let statement = parse_statements(
            "with c(x) as (select 42) select x as n from c",
            Dialect::sqlite(),
        )
        .next()
        .unwrap()?;
        let AstAndContextPair(ast, context) =
            SemAst::from_ast(&statement, context, &mut create_cache_context())?;
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
        let mut context = TestParseContext::default().with_dialect(Dialect::sqlite());
        let result =
            SemScalarExpr::from_ast(&syntax, &mut context, &ScalarAnalysisContext::default())?;
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
    #[cfg(feature = "sqlite")]
    use crate::test_helpers::SqliteSynBuilder;

    let parsed = SqliteSynBuilder.parse_expression(&format!("SELECT {name}"));
    let Expr::Parameter(parameter) = Expr::try_from(&parsed)? else {
        panic!("parameter");
    };
    assert_eq!(parameter.name().as_deref(), Some(name));
    assert_eq!(parameter.position(), 1);
    assert!(parameter.type_name().is_none());
    Ok(())
}

#[cfg(feature = "sqlite")]
#[test]
fn sqlite_upgrade_preserves_parameter_reuse() -> testresult::TestResult {
    use crate::ast::Expr;
    #[cfg(feature = "sqlite")]
    use crate::test_helpers::SqliteSynBuilder;

    let parsed =
        SqliteSynBuilder.parse_expression("SELECT $ns::name(element) + $ns::name(element)");
    let Expr::Operator(operator) = Expr::try_from(&parsed)? else {
        panic!("addition");
    };
    let Expr::Parameter(left) = operator.left().as_ref().unwrap() else {
        panic!("left parameter");
    };
    let Expr::Parameter(right) = operator.right().as_ref().unwrap() else {
        panic!("right parameter");
    };
    assert_eq!(left.name().as_deref(), Some("$ns::name(element)"));
    assert_eq!(right.name(), left.name());
    assert_eq!(left.position(), 1);
    assert_eq!(right.position(), left.position());
    Ok(())
}

#[cfg(feature = "sqlite")]
#[rstest::rstest]
fn sqlite_parenthesized_in_subqueries_keep_membership_semantics(
    #[values("(SELECT 2)", "((SELECT 2))", "(((SELECT 2)))")] rhs: &str,
) -> testresult::TestResult {
    use crate::ast::{BooleanOperator, Expr, Literal, SubqueryKind};
    #[cfg(feature = "sqlite")]
    use crate::test_helpers::SqliteSynBuilder;

    let parsed = SqliteSynBuilder.parse_expression(&format!("SELECT 1 IN {rhs}"));
    let Expr::Subquery(query) = Expr::try_from(&parsed)? else {
        panic!("membership subquery");
    };
    assert!(matches!(query.kind(), SubqueryKind::Any));
    assert!(matches!(
        query.test(),
        Some(Expr::Literal(Literal::Integer(1)))
    ));
    assert_eq!(
        FullName::from_ast_name(query.operator())?.name().as_str(),
        "="
    );
    let parsed = SqliteSynBuilder.parse_expression(&format!("SELECT 1 NOT IN {rhs}"));
    let Expr::Boolean(negation) = Expr::try_from(&parsed)? else {
        panic!("negated membership");
    };
    assert!(matches!(negation.operator(), BooleanOperator::Not));
    assert_eq!(negation.arguments().len(), 1);
    let Expr::Subquery(query) = &negation.arguments()[0] else {
        panic!("membership subquery");
    };
    assert!(matches!(query.kind(), SubqueryKind::Any));
    assert!(matches!(
        query.test(),
        Some(Expr::Literal(Literal::Integer(1)))
    ));
    Ok(())
}

#[cfg(feature = "sqlite")]
#[test]
fn sqlite_scalar_subquery_keeps_scalar_semantics() -> testresult::TestResult {
    use crate::ast::{Expr, SubqueryKind};
    #[cfg(feature = "sqlite")]
    use crate::test_helpers::SqliteSynBuilder;

    let parsed = SqliteSynBuilder.parse_expression("SELECT (SELECT 2)");
    let Expr::Subquery(query) = Expr::try_from(&parsed)? else {
        panic!("scalar subquery");
    };
    assert!(matches!(query.kind(), SubqueryKind::Scalar));
    assert!(query.test().is_none());
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
    #[cfg(feature = "sqlite")]
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
    SemAst::from_ast_node(&syntax, &mut catalog, &mut create_cache_context())?;
    assert_eq!(catalog.reported_problem_count(), 1);
    Ok(())
}

#[cfg(test)]
mod test_name_from_range_var {
    use crate::{
        ast::Name,
        names::{FullName, LocalName},
        syn::AliasOpt,
        test_helpers::SynBuilder,
    };

    #[rstest::rstest]
    #[case(FullName::new(&None, &None, &LocalName::from("items")))]
    #[case(FullName::with_schema("app", "items"))]
    #[case(FullName::with_catalog("catalog", "app", "items"))]
    fn preserves_relation_qualification(#[case] expected: FullName) -> testresult::TestResult {
        let source = SynBuilder::new().range_var(&expected, &AliasOpt::none());
        assert_eq!(
            FullName::from_ast_name(&Name::try_from(&source)?)?,
            expected
        );
        let optional = Some(Name::try_from(&source))
            .transpose()?
            .expect("present relation");
        assert_eq!(FullName::from_ast_name(&optional)?, expected);
        Ok(())
    }

    #[test]
    fn absent_relation_remains_absent() {
        assert!(
            None::<&crate::syn::RangeVar>
                .map(Name::try_from)
                .transpose()
                .unwrap()
                .is_none()
        );
    }
}

#[cfg(test)]
mod test_sqlite_row_comparisons {
    #[cfg(feature = "sqlite")]
    use crate::{ast::Dialect, test_helpers::query::resolve};

    #[cfg(feature = "sqlite")]
    #[rstest::rstest]
    #[case::membership("SELECT (1,2) IN (SELECT 1,2)")]
    #[case::is("SELECT (1,2) IS (1,2)")]
    fn sqlite_row_consumers_still_accept_row_operands(#[case] sql: &str) {
        let oracle = rusqlite::Connection::open_in_memory().unwrap();
        assert_eq!(
            oracle
                .query_row(sql, [], |row| row.get::<_, i32>(0))
                .unwrap(),
            1
        );
        let (_, context) = resolve(sql, Dialect::sqlite(), false).unwrap();
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:?}",
            context.reported_problems()
        );
    }
}

#[cfg(test)]
mod test_query_clause_diagnostics {
    use crate::{
        ast::Dialect,
        sem::{QueryNameResolution, QueryProblem as SemQueryProblem},
        test_helpers::query::resolve,
    };

    #[rstest::rstest]
    #[case::missing_operator("SELECT 1 ORDER BY 1 USING #", false)]
    #[case::missing_lock("SELECT n FROM (SELECT 1 AS n) s FOR UPDATE OF absent", true)]
    fn retains_failed_clause_name_bindings(#[case] sql: &str, #[case] lock: bool) {
        let (query, context) = resolve(sql, Dialect::postgres(), false).unwrap();
        assert_eq!(context.reported_problem_count(), 1);
        let boundary = query.query_clauses().unwrap();
        let outcome = if lock {
            boundary.locks()[0].relations()[0].resolution()
        } else {
            boundary.order()[0].using().unwrap().resolution()
        };
        assert_matches::assert_matches!(outcome, QueryNameResolution::NotFound);
    }

    #[rstest::rstest]
    #[case::duplicate_window(
            "SELECT 1 WINDOW w AS (), w AS ()",
            vec![crate::sem::QueryProblem::duplicate_window_definition("w")]
        )]
    #[case::missing_window(
            "SELECT sum(1) OVER absent",
            vec![crate::sem::QueryProblem::reference_not_found("absent")]
        )]
    #[case::ambiguous_window(
            "SELECT sum(1) OVER w WINDOW w AS (), w AS ()",
            vec![crate::sem::QueryProblem::duplicate_window_definition("w"), crate::sem::QueryProblem::ambiguous_reference("w")]
        )]
    #[case::missing_operator(
            "SELECT 1 ORDER BY 1 USING #",
            vec![crate::sem::QueryProblem::reference_not_found("#")]
        )]
    #[case::missing_lock_source(
            "SELECT n FROM (SELECT 1 AS n) s FOR UPDATE OF absent",
            vec![crate::sem::QueryProblem::reference_not_found("absent")]
        )]
    #[case::zero_position(
            "SELECT 1 ORDER BY 0",
            vec![crate::sem::QueryProblem::position_out_of_range(0)]
        )]
    #[case::group_position(
            "SELECT 1 GROUP BY 2",
            vec![crate::sem::QueryProblem::position_out_of_range(2)]
        )]
    #[case::compound_expression(
            "SELECT 1 UNION SELECT 2 ORDER BY 1 + 1",
            vec![crate::sem::QueryProblem::invalid_compound_order()]
        )]
    fn reports_structured_query_problems(
        #[case] sql: &str,
        #[case] expected: Vec<SemQueryProblem>,
    ) {
        let (_, context) = resolve(sql, Dialect::postgres(), false).unwrap();
        let mut actual = Vec::new();
        for problem in context.reported_problems() {
            if let Some(problem) = problem.query_problem() {
                actual.push(problem.clone());
            }
        }
        assert_eq!(actual, expected);
    }
}

#[cfg(test)]
mod test_wildcard_analysis_round_trip {
    use crate::{
        ast::Dialect,
        context::ParseContext,
        parse_statements,
        sem::SemAst,
        test_helpers::{create_cache_context, query::source_namespace_statement},
    };

    #[test]
    fn next_analysis_resets_previous_groups() {
        let mut cache_context = create_cache_context();
        let analyzed = source_namespace_statement("SELECT * FROM a.t");
        assert_eq!(
            analyzed
                .context()
                .analysis_state()
                .wildcard_expansions()
                .len(),
            1
        );
        let statement = parse_statements("SELECT 1", Dialect::postgres())
            .next()
            .unwrap()
            .unwrap();
        let next =
            SemAst::from_ast(&statement, analyzed.into_context(), &mut cache_context).unwrap();
        assert!(
            next.context()
                .analysis_state()
                .wildcard_expansions()
                .is_empty()
        );
    }

    #[rstest::rstest]
    #[case::table("SELECT j.*, $1::int4 AS parameter FROM a.mixed j(x,x)")]
    #[case::function("SELECT j.*, $1::int4 AS parameter FROM mixed_fun() j(x,x)")]
    fn duplicate_named_row_alias_rewrites_using_physical_fields(#[case] sql: &str) {
        let mut cache_context = create_cache_context();
        let original = source_namespace_statement(sql);
        let mut statement = parse_statements(sql, Dialect::postgres())
            .next()
            .unwrap()
            .unwrap();
        let analyzed =
            SemAst::from_ast(&statement, original.into_context(), &mut cache_context).unwrap();
        analyzed
            .context()
            .analysis_state()
            .wildcard_expansions()
            .rewrite(&mut statement)
            .unwrap();
        let output = statement.deparse_sql(Dialect::postgres()).unwrap();
        let reparsed = parse_statements(&output, Dialect::postgres())
            .next()
            .unwrap()
            .unwrap();
        let rewritten =
            SemAst::from_ast(&reparsed, analyzed.into_context(), &mut cache_context).unwrap();
        assert!(
            rewritten.context().reported_problems().is_empty(),
            "{:?}: {output}",
            rewritten.context().reported_problems()
        );
        let columns = rewritten.ast().result_columns().unwrap();
        assert_eq!(columns.len(), 3);
        assert_eq!(
            columns.get_column_at(0).unwrap().col_type(),
            &Some(crate::sem::PgBuiltInType::int4())
        );
        assert_eq!(
            columns.get_column_at(1).unwrap().col_type(),
            &Some(crate::sem::PgBuiltInType::int8())
        );
        assert_eq!(
            columns
                .get_column_at(0)
                .unwrap()
                .column_name()
                .as_ref()
                .unwrap()
                .as_str(),
            "x"
        );
        assert_eq!(
            columns
                .get_column_at(1)
                .unwrap()
                .column_name()
                .as_ref()
                .unwrap()
                .as_str(),
            "x"
        );
        assert!(
            rewritten
                .context()
                .parameter_constraints()
                .parameter(1)
                .is_some()
        );
    }
}

#[cfg(test)]
mod test_public_postgres_entry_points_share_insert_resolution {
    #[cfg(feature = "sqlite")]
    use crate::test_helpers::insert::analyze;

    use crate::{ast::Dialect, context::ParseContext, test_helpers::insert as test_helpers};

    #[test]
    fn public_postgres_entry_points_share_insert_resolution() {
        use crate::{parse_statements, sem::SemAst, test_helpers::create_cache_context};
        let sql = "INSERT INTO items(id) VALUES($1) RETURNING id";
        let statement = parse_statements(sql, Dialect::postgres())
            .next()
            .unwrap()
            .unwrap();
        let result = SemAst::from_ast(
            &statement,
            test_helpers::catalog(Dialect::postgres()),
            &mut create_cache_context(),
        )
        .unwrap();
        assert_eq!(result.ast().as_insert().unwrap().result_columns().len(), 1);
        assert!(
            result
                .context()
                .parameter_constraints()
                .parameter(1)
                .is_some()
        );
        let mut context = test_helpers::catalog(Dialect::postgres());
        let result =
            SemAst::from_ast_node(statement.node(), &mut context, &mut create_cache_context())
                .unwrap();
        assert_eq!(result.as_insert().unwrap().result_columns().len(), 1);
        assert!(context.parameter_constraints().parameter(1).is_some());
    }

    #[cfg(feature = "sqlite")]
    #[rstest::rstest]
    #[case::values("INSERT INTO items VALUES(1,'one') RETURNING *", true)]
    #[case::short_values("INSERT INTO items VALUES(1)", false)]
    #[case::explicit_column("INSERT INTO items(id) VALUES(1) RETURNING id", true)]
    #[case::duplicates("INSERT INTO items(id,id) VALUES(1,2)", true)]
    #[case::qualified_original("INSERT INTO items AS i(id) VALUES(1) RETURNING items.id", true)]
    #[case::qualified_alias("INSERT INTO items AS i(id) VALUES(1) RETURNING i.id", false)]
    #[case::qualified_star("INSERT INTO items(id) VALUES(1) RETURNING items.*", false)]
    #[case::affinity("INSERT INTO items(id) VALUES('one') RETURNING id", true)]
    fn agrees_with_bundled_sqlite(#[case] sql: &str, #[case] accepted: bool) {
        let oracle = ::rusqlite::Connection::open_in_memory().unwrap();
        oracle
            .execute_batch("CREATE TABLE items(id INTEGER, label TEXT)")
            .unwrap();
        assert_eq!(
            oracle.prepare(sql).is_ok(),
            accepted,
            "SQLite {}: {sql}",
            ::rusqlite::version()
        );
        let (_, context) = analyze(sql, Dialect::sqlite());
        assert_eq!(
            context.reported_problem_count() == 0,
            accepted,
            "{:#?}",
            context.reported_problems()
        );
    }

    #[cfg(feature = "sqlite")]
    #[test]
    fn public_sqlite_entry_point_retains_returning_and_parameters() {
        #[cfg(feature = "sqlite")]
        use crate::{parse_statements, sem::SemAst, test_helpers::create_cache_context};
        let sql = "INSERT INTO items(id) VALUES(?1) RETURNING id";
        let statement = parse_statements(sql, Dialect::sqlite())
            .next()
            .unwrap()
            .unwrap();
        let result = SemAst::from_ast(
            &statement,
            test_helpers::catalog(Dialect::sqlite()),
            &mut create_cache_context(),
        )
        .unwrap();
        assert_eq!(result.ast().as_insert().unwrap().result_columns().len(), 1);
        assert!(
            result
                .context()
                .parameter_constraints()
                .parameter(1)
                .unwrap()
                .constraint()
                .is_any()
        );
    }
}

#[cfg(test)]
mod test_rejects_invalid_structure {
    use crate::{
        ast::Dialect,
        context::ParseContext,
        parse_statements,
        sem::SemAst,
        test_helpers::{create_cache_context, update as test_helpers, update::catalog},
    };

    #[cfg(feature = "sqlite")]
    use crate::sem::SemAst as SemSemAst;

    #[rstest::rstest]
    #[case::missing_relation("UPDATE absent SET id=1", false)]
    #[case::missing_column("UPDATE items SET absent=1", false)]
    #[case::row_width("UPDATE items SET (id,label)=(SELECT 1)", true)]
    #[case::unreadable("WITH u AS (UPDATE items SET id=1) SELECT * FROM u", true)]
    fn rejects_invalid_structure(#[case] sql: &str, #[case] fatal: bool) {
        let mut context = catalog(Dialect::postgres());
        let statement = parse_statements(sql, Dialect::postgres())
            .next()
            .unwrap()
            .unwrap();
        let result =
            SemAst::from_ast_node(statement.node(), &mut context, &mut create_cache_context());
        if fatal {
            assert!(result.is_err());
        } else {
            assert!(matches!(
                result.unwrap(),
                crate::sem::SemAst::UnresolvedStatement
            ));
            assert!(context.reported_problem_count() > 0);
        }
    }

    #[test]
    fn public_postgres_entry_points_share_update_resolution() {
        let sql = "UPDATE items SET id=$1 RETURNING id";
        let statement = parse_statements(sql, Dialect::postgres())
            .next()
            .unwrap()
            .unwrap();
        let result = SemAst::from_ast(
            &statement,
            test_helpers::catalog(Dialect::postgres()),
            &mut create_cache_context(),
        )
        .unwrap();
        assert_eq!(result.ast().as_update().unwrap().result_columns().len(), 1);
        assert!(
            result
                .context()
                .parameter_constraints()
                .parameter(1)
                .is_some()
        );
        let mut context = test_helpers::catalog(Dialect::postgres());
        let result =
            SemAst::from_ast_node(statement.node(), &mut context, &mut create_cache_context())
                .unwrap();
        assert_eq!(result.as_update().unwrap().result_columns().len(), 1);
        assert!(context.parameter_constraints().parameter(1).is_some());
    }

    #[cfg(feature = "sqlite")]
    #[test]
    fn public_sqlite_entry_point_analyzes_update() {
        let syntax = parse_statements(
            "UPDATE OR IGNORE items NOT INDEXED SET id=?1 RETURNING id",
            Dialect::sqlite(),
        )
        .next()
        .unwrap()
        .unwrap();
        let result = SemSemAst::from_ast(
            &syntax,
            catalog(Dialect::sqlite()),
            &mut create_cache_context(),
        )
        .unwrap();
        let update = result.ast().as_update().unwrap();
        assert!(update.or_conflict().is_some());
        assert!(update.not_indexed());
        assert_eq!(update.result_columns().len(), 1);
        assert!(
            result
                .context()
                .parameter_constraints()
                .parameter(1)
                .is_some()
        );
    }
}

#[cfg(test)]
mod test_retains_invalid_structure_as_diagnostics {
    #[cfg(feature = "sqlite")]
    use crate::sem::SemAst as SemSemAst;
    #[cfg(feature = "sqlite")]
    use crate::{parse_statements, test_helpers::create_cache_context};

    use crate::{
        ast::Dialect,
        context::ParseContext,
        test_helpers::{delete as test_helpers, delete::catalog},
    };

    #[rstest::rstest]
    #[case::missing_target("DELETE FROM absent")]
    #[case::unreadable("WITH d AS (DELETE FROM items) SELECT * FROM d")]
    #[case::nested("SELECT (WITH d AS (DELETE FROM items RETURNING id) SELECT id FROM d)")]
    fn retains_invalid_structure_as_diagnostics(#[case] sql: &str) {
        let mut context = catalog(Dialect::postgres());
        assert!(test_helpers::resolve(sql, &mut context, Dialect::postgres()).is_ok());
        assert!(context.reported_problem_count() > 0);
    }

    #[test]
    fn postgres_public_entry_points_agree() {
        use crate::{parse_statements, sem::SemAst, test_helpers::create_cache_context};
        let sql = "DELETE FROM items WHERE $1 RETURNING id";
        let statement = parse_statements(sql, Dialect::postgres())
            .next()
            .unwrap()
            .unwrap();
        let result = SemAst::from_ast(
            &statement,
            catalog(Dialect::postgres()),
            &mut create_cache_context(),
        )
        .unwrap();
        let mut context = catalog(Dialect::postgres());
        let adapted =
            SemAst::from_ast_node(statement.node(), &mut context, &mut create_cache_context())
                .unwrap();
        assert_eq!(result.ast().as_delete(), adapted.as_delete());
        assert_eq!(
            result
                .context()
                .parameter_constraints()
                .parameter(1)
                .unwrap()
                .constraint(),
            context
                .parameter_constraints()
                .parameter(1)
                .unwrap()
                .constraint()
        );
    }

    #[cfg(feature = "sqlite")]
    #[test]
    fn sqlite_public_adapter_analyzes_delete() {
        let syntax = parse_statements(
            "DELETE FROM items NOT INDEXED WHERE ?1 RETURNING id",
            Dialect::sqlite(),
        )
        .next()
        .unwrap()
        .unwrap();
        let result = SemSemAst::from_ast(
            &syntax,
            catalog(Dialect::sqlite()),
            &mut create_cache_context(),
        )
        .unwrap();
        let delete = result.ast().as_delete().unwrap();
        assert!(delete.not_indexed());
        assert_eq!(delete.result_columns().len(), 1);
        assert!(
            result
                .context()
                .parameter_constraints()
                .parameter(1)
                .is_some()
        );
    }
}

#[cfg(test)]
mod test_sql_driven_delete_cte_preserves_bindings_types_and_schema {
    use crate::{
        ast::Dialect,
        context::{BaseParseContext, ParseContext},
        names::{AliasName, ColumnName, FullName, TableName},
        parse_statements,
        sem::{CteStatement, DataSource, PgBuiltInType, SelectStatement, SemAst},
        test_helpers::{create_cache_context, delete_sql_driven as test_helpers},
    };

    #[test]
    fn sql_driven_delete_cte_preserves_bindings_types_and_schema()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut cache_context = create_cache_context();
        let mut context = test_helpers::context();
        for statement in parse_statements(
            "CREATE TABLE items(id integer NOT NULL, label text); CREATE TABLE sources(id integer, label text)",
            Dialect::postgres(),
        ) {
            context = SemAst::from_ast(&statement?, context, &mut cache_context)?.into_context();
        }
        let statement = parse_statements("WITH d(removed,label) AS (DELETE FROM items AS i USING sources AS s WHERE i.id=s.id AND i.id=$1 RETURNING i.id,s.label) SELECT removed,label FROM d", Dialect::postgres()).next().unwrap()?;
        let analyzed = SemAst::from_ast(&statement, context, &mut cache_context)?;
        assert!(
            analyzed.context().reported_problems().is_empty(),
            "{:?}",
            analyzed.context().reported_problems()
        );
        let constraints = analyzed.context().parameter_constraints();
        assert!(
            constraints
                .parameter(1)
                .unwrap()
                .constraint()
                .allows(&PgBuiltInType::int4())
        );
        assert!(
            !constraints
                .parameter(1)
                .unwrap()
                .constraint()
                .allows(&PgBuiltInType::bool())
        );
        let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
        let SelectStatement::Query(boundary) = &query else {
            panic!("query boundary")
        };
        let CteStatement::Delete(delete) = boundary.with()[0].statement() else {
            panic!("DELETE CTE")
        };
        assert_eq!(delete.target().to_string(), "public.items");
        for (alias, relation) in [("i", "items"), ("s", "sources")] {
            let source = delete
                .from()
                .resolve_alias(&AliasName::from(alias))
                .unwrap()
                .0;
            let DataSource::Table(table) = source else {
                panic!("catalog table")
            };
            let expected = TableName::from(FullName::with_schema("public", relation));
            assert_eq!(
                analyzed.context().get_table(table.name()).unwrap().name(),
                analyzed.context().get_table(&expected).unwrap().name()
            );
        }
        assert!(delete.predicate().is_some());
        let source_column = delete.result_columns().get_column_at(1).unwrap();
        assert_eq!(source_column.col_type(), &Some(PgBuiltInType::text()));
        assert_eq!(source_column.is_not_null(), &Some(false));
        assert!(
            source_column
                .expression()
                .unwrap()
                .get_column_def()
                .is_some()
        );
        let column = query.result_columns().get_column_at(0).unwrap();
        assert_eq!(column.column_name(), &Some(ColumnName::from("removed")));
        assert_eq!(column.col_type(), &Some(PgBuiltInType::int4()));
        assert_eq!(column.is_not_null(), &Some(true));
        assert!(
            analyzed
                .context()
                .get_table(&TableName::from(FullName::with_schema("public", "items")))
                .is_some()
        );
        Ok(())
    }
}

#[cfg(test)]
mod test_accepts_shared_default_assignment_rules {
    use crate::test_helpers::{
        merge_assignments::{assert_rejected, assignment_catalog},
        statements::resolve_postgres as resolve,
    };

    #[rstest::rstest]
    #[case::identity_update_default(
        "MERGE INTO identity_items t USING incoming s ON true WHEN MATCHED THEN UPDATE SET id=DEFAULT"
    )]
    #[case::generated_update_default(
        "MERGE INTO generated_items t USING incoming s ON true WHEN MATCHED THEN UPDATE SET g=DEFAULT"
    )]
    #[case::generated_insert_default(
        "MERGE INTO generated_items t USING incoming s ON true WHEN NOT MATCHED THEN INSERT(g) VALUES(DEFAULT)"
    )]
    #[case::insert_default_values(
        "MERGE INTO identity_items t USING incoming s ON true WHEN NOT MATCHED THEN INSERT DEFAULT VALUES"
    )]
    fn accepts_shared_default_assignment_rules(#[case] sql: &str) {
        let mut context = assignment_catalog(17);
        let statement = resolve(sql, &mut context).unwrap();
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:?}",
            context.reported_problems()
        );
        assert!(statement.as_merge().is_some());
    }

    #[rstest::rstest]
    #[case::identity_update_value(
        "MERGE INTO identity_items t USING incoming s ON true WHEN MATCHED THEN UPDATE SET id=s.sid"
    )]
    #[case::generated_update_value(
        "MERGE INTO generated_items t USING incoming s ON true WHEN MATCHED THEN UPDATE SET g=s.sid"
    )]
    #[case::identity_insert_value(
        "MERGE INTO identity_items t USING incoming s ON true WHEN NOT MATCHED THEN INSERT(id) VALUES(s.sid)"
    )]
    #[case::generated_insert_value(
        "MERGE INTO generated_items t USING incoming s ON true WHEN NOT MATCHED THEN INSERT(g) VALUES(s.sid)"
    )]
    fn rejects_shared_identity_and_generated_value_rules(#[case] sql: &str) {
        assert_rejected(sql, 17);
    }

    #[rstest::rstest]
    #[case::duplicate_update(
        "MERGE INTO items t USING incoming s ON true WHEN MATCHED THEN UPDATE SET id=s.sid,id=s.sid"
    )]
    #[case::duplicate_insert(
        "MERGE INTO items t USING incoming s ON true WHEN NOT MATCHED THEN INSERT(id,id) VALUES(s.sid,s.sid)"
    )]
    #[case::update_type_mismatch(
        "MERGE INTO items t USING incoming s ON true WHEN MATCHED THEN UPDATE SET id=s.slabel"
    )]
    #[case::insert_type_mismatch(
        "MERGE INTO items t USING incoming s ON true WHEN NOT MATCHED THEN INSERT(id) VALUES(s.slabel)"
    )]
    fn rejects_duplicate_targets_and_type_mismatches(#[case] sql: &str) {
        assert_rejected(sql, 17);
    }
}

#[cfg(test)]
mod test_insert_and_update_assignments_use_their_action_specific_writable_columns {
    use crate::{
        sem::{ViewMergeSupport, ViewMergeTriggers},
        test_helpers::{
            merge_views::{columns, view_catalog},
            statements::resolve_postgres as resolve,
        },
    };

    #[test]
    fn insert_and_update_assignments_use_their_action_specific_writable_columns() {
        let support = ViewMergeSupport::new(
            true,
            false,
            ViewMergeTriggers::default(),
            Some(&columns(&["id"])),
            Some(&columns(&["label"])),
        );

        let mut context = view_catalog(17, &support);
        let insert = resolve(
            "MERGE INTO writable_items t USING incoming s ON true WHEN NOT MATCHED THEN INSERT(id) VALUES(s.sid)",
            &mut context,
        );
        assert!(
            insert.is_ok(),
            "{insert:?} {:?}",
            context.reported_problems()
        );

        let mut context = view_catalog(17, &support);
        let insert_read_only = resolve(
            "MERGE INTO writable_items t USING incoming s ON true WHEN NOT MATCHED THEN INSERT(label) VALUES(s.slabel)",
            &mut context,
        );
        assert!(insert_read_only.is_err() || context.reported_problem_count() > 0);

        let mut context = view_catalog(17, &support);
        let update = resolve(
            "MERGE INTO writable_items t USING incoming s ON true WHEN MATCHED THEN UPDATE SET label=s.slabel",
            &mut context,
        );
        assert!(
            update.is_ok(),
            "{update:?} {:?}",
            context.reported_problems()
        );

        let mut context = view_catalog(17, &support);
        let update_read_only = resolve(
            "MERGE INTO writable_items t USING incoming s ON true WHEN MATCHED THEN UPDATE SET id=s.sid",
            &mut context,
        );
        assert!(update_read_only.is_err() || context.reported_problem_count() > 0);
    }
}

#[cfg(test)]
mod test_public_statement_entry_point_analyzes_and_deparses_merge {
    use crate::{
        ast::{CteDefinition, Dialect, HaveWithClause, StatementKind},
        sem::SemAst,
        test_helpers::{
            create_cache_context, document_merge::catalog, merge_statements::statement,
        },
    };

    #[test]
    fn public_statement_entry_point_analyzes_and_deparses_merge() {
        let mut cache_context = create_cache_context();
        let source = statement(
            "MERGE INTO items AS t USING incoming AS s ON true WHEN MATCHED THEN UPDATE SET label = s.slabel RETURNING t.id",
        );
        assert!(source.is_merge());
        assert_eq!(source.kind(), StatementKind::merge());

        let analyzed =
            SemAst::from_ast(&source, catalog(17), &mut cache_context).expect("MERGE analysis");
        assert!(analyzed.ast().as_merge().is_some());
        assert_eq!(analyzed.context().reported_problem_count(), 0);

        let rendered = source
            .deparse_sql(Dialect::postgres())
            .expect("MERGE deparse");
        let reparsed = statement(&rendered);
        assert!(reparsed.is_merge());
        assert_eq!(reparsed.deparse_sql(Dialect::postgres()).unwrap(), rendered);
    }

    #[test]
    fn merge_cte_body_can_be_inserted_and_roundtripped() {
        let merge = statement(
            "MERGE INTO items AS t USING incoming AS s ON true WHEN MATCHED THEN UPDATE SET label = s.slabel RETURNING t.id",
        );
        let mut definition = CteDefinition::new("changed", &merge).expect("MERGE CTE body");
        definition
            .set_columns(&["renamed_id"])
            .expect("CTE output alias");

        let mut query = statement("SELECT renamed_id FROM changed");
        query
            .ensure_with_clause()
            .expect("WITH editor")
            .insert(0, &definition)
            .expect("insert MERGE CTE");

        let cte = query.with_clause().unwrap().find("changed").unwrap();
        assert_eq!(cte.body_kind(), StatementKind::merge());
        assert_eq!(cte.columns().collect::<Vec<_>>(), vec!["renamed_id"]);

        let rendered = query
            .deparse_sql(Dialect::postgres())
            .expect("query with MERGE CTE deparse");
        let reparsed = statement(&rendered);
        let cte = reparsed.with_clause().unwrap().find("changed").unwrap();
        assert_eq!(cte.body_kind(), StatementKind::merge());
        assert_eq!(cte.columns().collect::<Vec<_>>(), vec!["renamed_id"]);
    }

    #[test]
    fn merge_statement_with_clause_can_be_mutated_and_deparsed() {
        let mut cache_context = create_cache_context();
        let mut merge = statement(
            "MERGE INTO items AS t USING incoming AS s ON true WHEN MATCHED THEN UPDATE SET label = s.slabel",
        );
        let definition = CteDefinition::new(
            "incoming",
            &statement("SELECT 1 AS sid, 'source' AS slabel"),
        )
        .expect("source CTE");
        merge
            .ensure_with_clause()
            .expect("MERGE WITH editor")
            .insert(0, &definition)
            .expect("insert source CTE");

        assert_eq!(merge.with_clause().unwrap().len(), 1);
        let rendered = merge
            .deparse_sql(Dialect::postgres())
            .expect("MERGE with CTE deparse");
        let reparsed = statement(&rendered);
        assert!(reparsed.is_merge());
        assert_eq!(reparsed.with_clause().unwrap().len(), 1);
        assert!(SemAst::from_ast(&reparsed, catalog(17), &mut cache_context).is_ok());
    }
}

#[cfg(test)]
mod test_ownership_lifecycle {
    use crate::{
        ast::ObjectKind,
        context::{BaseParseContext, OwnershipInfoRead, ParseContext, TableInfoRead, TypeInfoRead},
        names::{FullName, TableName},
        sem::{
            OwnedObjectDefinition, OwnerSpecification, OwnershipIdentity,
            PgBuiltInType as SemPgBuiltInType, Role,
        },
        test_helpers::{
            ownership::{apply_ownership_sql, ownership_context},
            schema_layer,
        },
    };

    #[rstest::rstest]
    #[case::aggregate(2, "AGGREGATE public.total(integer)", "total", true, None)]
    #[case::collation(8, "COLLATION public.c", "c", false, None)]
    #[case::conversion(9, "CONVERSION public.c", "c", false, None)]
    #[case::database(10, "DATABASE db", "db", false, None)]
    #[case::event_trigger(15, "EVENT TRIGGER trigger1", "trigger1", false, None)]
    #[case::fdw(17, "FOREIGN DATA WRAPPER wrapper", "wrapper", false, None)]
    #[case::server(18, "SERVER foreign1", "foreign1", false, None)]
    #[case::language(22, "LANGUAGE lang", "lang", false, None)]
    #[case::large_object(23, "LARGE OBJECT 12345", "12345", false, None)]
    #[case::operator_class(
        25,
        "OPERATOR CLASS public.opclass USING btree",
        "opclass",
        false,
        Some("btree")
    )]
    #[case::operator_family(
        27,
        "OPERATOR FAMILY public.opfamily USING hash",
        "opfamily",
        false,
        Some("hash")
    )]
    #[case::publication(31, "PUBLICATION pub1", "pub1", false, None)]
    #[case::statistics(40, "STATISTICS public.stats", "stats", false, None)]
    #[case::subscription(39, "SUBSCRIPTION sub1", "sub1", false, None)]
    #[case::tablespace(43, "TABLESPACE space1", "space1", false, None)]
    #[case::text_configuration(46, "TEXT SEARCH CONFIGURATION public.cfg", "cfg", false, None)]
    #[case::text_dictionary(47, "TEXT SEARCH DICTIONARY public.dict", "dict", false, None)]
    fn resolves_and_applies_catalog_object_classes(
        #[case] kind: u32,
        #[case] target: &str,
        #[case] local: &str,
        #[case] signed: bool,
        #[case] method: Option<&str>,
    ) -> testresult::TestResult {
        let mut context = ownership_context();
        let kind = ObjectKind::from(kind);
        let name = if OwnershipIdentity::owner_schema_scoped(kind) {
            FullName::with_schema("public", local)
        } else {
            FullName::from_local_name(local)
        };
        let identity = if signed {
            OwnershipIdentity::with_arguments(kind, &name, &[Some(SemPgBuiltInType::int4())])
        } else if let Some(method) = method {
            OwnershipIdentity::with_access_method(kind, &name, method)
        } else {
            OwnershipIdentity::named(kind, &name)
        };
        context.insert_owned_object(&OwnedObjectDefinition::new(&identity, None));
        let result = apply_ownership_sql(context, &format!("ALTER {target} OWNER TO alice"))?;
        let change = result
            .ast()
            .ownership_change()
            .expect("ownership semantics");
        assert_eq!(change.target().identity(), Some(&identity));
        assert_eq!(
            change.owner().resolved_role().map(Role::name),
            Some("alice")
        );
        let definitions = result.context().ownership_definitions(kind, &name);
        assert_eq!(definitions.len(), 1);
        assert_eq!(definitions[0].owner(), Some(change.owner()));
        let frozen = result.into_context().freeze();
        let next = schema_layer(std::path::Path::new("next.sql"), &frozen);
        assert_eq!(next.ownership_definitions(kind, &name), definitions);
        Ok(())
    }

    #[rstest::rstest]
    #[case::domain("CREATE DOMAIN public.d AS integer", "DOMAIN public.d", 13, "d")]
    #[case::enum_type("CREATE TYPE public.e AS ENUM ('x')", "TYPE public.e", 50, "e")]
    #[case::composite_type("CREATE TYPE public.c AS (value integer)", "TYPE public.c", 50, "c")]
    #[case::schema("CREATE SCHEMA app", "SCHEMA app", 37, "app")]
    #[case::function(
        "CREATE FUNCTION public.f(integer) RETURNS integer LANGUAGE sql AS 'SELECT $1'",
        "FUNCTION public.f(integer)",
        20,
        "f"
    )]
    #[case::procedure(
        "CREATE PROCEDURE public.p(INOUT value integer) LANGUAGE sql AS 'SELECT value'",
        "PROCEDURE public.p(integer)",
        30,
        "p"
    )]
    #[case::routine(
        "CREATE FUNCTION public.r() RETURNS integer LANGUAGE sql AS 'SELECT 1'",
        "ROUTINE public.r()",
        20,
        "r"
    )]
    fn changes_retained_ddl_models(
        #[case] create: &str,
        #[case] target: &str,
        #[case] kind: u32,
        #[case] local: &str,
    ) -> testresult::TestResult {
        let context = apply_ownership_sql(ownership_context(), create)?.into_context();
        let result = apply_ownership_sql(context, &format!("ALTER {target} OWNER TO bob"))?;
        assert!(
            result.context().problems().is_empty(),
            "{:?}",
            result.context().problems()
        );
        let kind = ObjectKind::from(kind);
        let name = if OwnershipIdentity::owner_schema_scoped(kind) {
            FullName::with_schema("public", local)
        } else {
            FullName::from_local_name(local)
        };
        let definitions = result.context().ownership_definitions(kind, &name);
        assert_eq!(definitions.len(), 1);
        assert_eq!(
            definitions[0]
                .owner()
                .unwrap()
                .resolved_role()
                .map(Role::name),
            Some("bob")
        );
        if kind.code() == 50 || kind.code() == 13 {
            assert_eq!(
                result.context().get_type_impl(&name).unwrap().owner(),
                definitions[0].owner()
            );
        }
        if kind.code() == 20 || kind.code() == 30 {
            assert_eq!(
                result.context().routine_definitions(&name)[0].owner(),
                definitions[0].owner()
            );
        }
        Ok(())
    }

    #[rstest::rstest]
    #[case::table("CREATE TABLE public.t (id integer)", "TABLE", 42)]
    #[case::view("CREATE VIEW public.t AS SELECT 1 AS id", "VIEW", 52)]
    #[case::sequence("CREATE SEQUENCE public.t", "SEQUENCE", 38)]
    #[case::materialized(
        "CREATE MATERIALIZED VIEW public.t AS SELECT 1 AS id",
        "MATERIALIZED VIEW",
        24
    )]
    fn relation_commands_keep_their_own_semantic_path(
        #[case] create: &str,
        #[case] target: &str,
        #[case] kind: u32,
    ) -> testresult::TestResult {
        let context = apply_ownership_sql(ownership_context(), create)?.into_context();
        let result = apply_ownership_sql(
            context,
            &format!("ALTER {target} public.t OWNER TO CURRENT_USER"),
        )?;
        assert!(result.ast().ownership_change().is_none());
        assert!(
            result.context().problems().is_empty(),
            "{:?}",
            result.context().problems()
        );
        let definitions = result.context().ownership_definitions(
            ObjectKind::from(kind),
            &FullName::with_schema("public", "t"),
        );
        assert_eq!(definitions.len(), 1);
        assert_eq!(
            definitions[0].owner(),
            Some(&OwnerSpecification::current_user())
        );
        Ok(())
    }

    #[test]
    fn missing_roles_and_targets_are_explicit_and_never_apply() -> testresult::TestResult {
        let context = apply_ownership_sql(ownership_context(), "CREATE SCHEMA app")?.into_context();
        let result = apply_ownership_sql(context, "ALTER SCHEMA app OWNER TO absent")?;
        let alteration = result.ast().ownership_change().unwrap();
        assert!(alteration.target().identity().is_some());
        assert_eq!(alteration.owner().name(), Some("absent"));
        assert!(alteration.owner().resolved_role().is_none());
        assert!(
            result
                .context()
                .schema_definition("app")
                .unwrap()
                .owner()
                .is_none()
        );
        assert_eq!(
            result.context().problems()[0].code(),
            "ownership_role_not_found"
        );
        assert_eq!(
            result.context().problems()[0]
                .ownership_problem()
                .unwrap()
                .role_name(),
            Some("absent")
        );
        let result = apply_ownership_sql(
            result.into_context(),
            "ALTER SCHEMA missing OWNER TO another_absent",
        )?;
        assert_eq!(
            result.ast().ownership_change().unwrap().target().code(),
            "ownership_target_not_found"
        );
        assert_eq!(result.context().problems().len(), 3);
        let result = apply_ownership_sql(result.into_context(), "ALTER SCHEMA app OWNER TO alice")?;
        assert_eq!(
            result
                .context()
                .schema_definition("app")
                .unwrap()
                .owner()
                .unwrap()
                .resolved_role()
                .map(Role::name),
            Some("alice")
        );
        Ok(())
    }

    #[rstest::rstest]
    #[case::ambiguous("ALTER FUNCTION public.f OWNER TO alice", "ownership_target_ambiguous")]
    #[case::no_implicit_cast(
        "ALTER FUNCTION public.f(bigint) OWNER TO alice",
        "ownership_target_not_found"
    )]
    #[case::missing_type(
        "ALTER FUNCTION public.f(absent) OWNER TO alice",
        "ownership_argument_type_not_found"
    )]
    #[case::wrong_kind(
        "ALTER PROCEDURE public.f(integer) OWNER TO alice",
        "ownership_invalid_target_kind"
    )]
    #[case::empty_signature(
        "ALTER FUNCTION public.f() OWNER TO alice",
        "ownership_target_not_found"
    )]
    fn invalid_overload_lookups_preserve_catalog(
        #[case] sql: &str,
        #[case] expected: &str,
    ) -> testresult::TestResult {
        let context = apply_ownership_sql(
            ownership_context(),
            "CREATE FUNCTION public.f(integer) RETURNS integer LANGUAGE sql AS 'SELECT $1'",
        )?
        .into_context();
        let context = apply_ownership_sql(
            context,
            "CREATE FUNCTION public.f(text) RETURNS text LANGUAGE sql AS 'SELECT $1'",
        )?
        .into_context();
        let result = apply_ownership_sql(context, sql)?;
        assert_eq!(
            result.ast().ownership_change().unwrap().target().code(),
            expected
        );
        assert_eq!(result.context().problems()[0].code(), expected);
        assert_eq!(
            crate::test_helpers::ownership::known_owner_count(
                &result.context().ownership_definitions(
                    ObjectKind::from(20),
                    &FullName::with_schema("public", "f")
                )
            ),
            0
        );
        Ok(())
    }

    #[test]
    fn table_row_type_reads_follow_the_relation_owner_and_cannot_be_altered_independently()
    -> testresult::TestResult {
        let context =
            apply_ownership_sql(ownership_context(), "CREATE TABLE public.t (id integer)")?
                .into_context();
        let context =
            apply_ownership_sql(context, "ALTER TABLE public.t OWNER TO alice")?.into_context();
        let name = FullName::with_schema("public", "t");
        let definitions = context.ownership_definitions(ObjectKind::from(50), &name);
        assert_eq!(
            definitions[0].owner(),
            context.get_table_impl(&name).unwrap().owner()
        );
        let result = apply_ownership_sql(context, "ALTER TYPE public.t OWNER TO bob")?;
        assert_eq!(
            result.ast().ownership_change().unwrap().target().code(),
            "ownership_invalid_target_kind"
        );
        assert_eq!(
            result
                .context()
                .get_table(&TableName::from(name))
                .unwrap()
                .owner()
                .unwrap()
                .resolved_role()
                .map(Role::name),
            Some("alice")
        );
        Ok(())
    }

    #[rstest::rstest]
    #[case::table("CREATE TABLE public.t (id integer)", "TABLE", 42)]
    #[case::view("CREATE VIEW public.t AS SELECT 1 AS id", "VIEW", 52)]
    #[case::sequence("CREATE SEQUENCE public.t", "SEQUENCE", 38)]
    fn unqualified_relation_changes_replace_the_matched_qualified_key(
        #[case] create: &str,
        #[case] kind: &str,
        #[case] code: u32,
    ) -> testresult::TestResult {
        let context = apply_ownership_sql(ownership_context(), create)?.into_context();
        let result = apply_ownership_sql(context, &format!("ALTER {kind} t OWNER TO alice"))?;
        let definitions = result.context().ownership_definitions(
            ObjectKind::from(code),
            &FullName::with_schema("public", "t"),
        );
        assert_eq!(
            definitions[0]
                .owner()
                .unwrap()
                .resolved_role()
                .map(Role::name),
            Some("alice")
        );
        Ok(())
    }
}

#[cfg(test)]
mod test_ownership_adapter_equivalence {
    use crate::{
        ast::StatementNode,
        context::OwnershipInfoRead,
        sem::{AlterOwner, SemAst},
        test_helpers::{
            create_cache_context,
            ownership::{apply_ownership_sql, ownership_context, ownership_statement},
        },
    };

    #[test]
    fn resolution_does_not_apply_and_legacy_entry_point_shares_the_result() -> testresult::TestResult
    {
        let mut context =
            apply_ownership_sql(ownership_context(), "CREATE SCHEMA app")?.into_context();
        let statement = ownership_statement("ALTER SCHEMA app OWNER TO alice")?;
        let semantics =
            SemAst::from_ast_node(&statement, &mut context, &mut create_cache_context())?;
        assert!(
            semantics
                .ownership_change()
                .unwrap()
                .target()
                .identity()
                .is_some()
        );
        assert!(context.schema_definition("app").unwrap().owner().is_none());
        let StatementNode::AlterOwner(syntax) = &statement else {
            panic!("ALTER OWNER syntax")
        };
        let result = AlterOwner::from_ast_and_apply(syntax, context)?;
        assert_eq!(
            result.ast().ownership_change().unwrap().target(),
            semantics.ownership_change().unwrap().target()
        );
        assert_eq!(
            result.context().schema_definition("app").unwrap().owner(),
            Some(semantics.ownership_change().unwrap().owner())
        );
        Ok(())
    }
}

#[cfg(test)]
mod test_schema_authorization_lifecycle {
    use crate::{
        ast::ObjectKind,
        context::OwnershipInfoRead,
        names::FullName,
        sem::Role,
        test_helpers::ownership::{apply_ownership_sql, ownership_context},
    };

    #[rstest::rstest]
    #[case::named("alice", Some("alice"), None, 0)]
    #[case::missing("missing", None, None, 1)]
    #[case::current("CURRENT_USER", None, Some("CURRENT_USER"), 0)]
    fn schema_authorization_preserves_valid_owners_and_rejects_missing_roles(
        #[case] authorization: &str,
        #[case] role_name: Option<&str>,
        #[case] keyword: Option<&str>,
        #[case] diagnostic_count: usize,
    ) -> testresult::TestResult {
        let result = apply_ownership_sql(
            ownership_context(),
            &format!("CREATE SCHEMA app AUTHORIZATION {authorization}"),
        )?;
        assert_eq!(result.context().problems().len(), diagnostic_count);
        if diagnostic_count != 0 {
            assert!(result.context().schema_definition("app").is_none());
            assert!(
                result
                    .context()
                    .ownership_definitions(ObjectKind::SCHEMA, &FullName::from_local_name("app"))
                    .is_empty()
            );
            assert_eq!(
                result.context().problems()[0]
                    .ownership_problem()
                    .unwrap()
                    .role_name(),
                Some(authorization)
            );
            return Ok(());
        }
        let schema = result.context().schema_definition("app").unwrap();
        let owner = schema.owner().unwrap();
        assert_eq!(owner.resolved_role().map(Role::name), role_name);
        assert_eq!(owner.session_keyword(), keyword);
        if keyword.is_none() {
            assert_eq!(owner.name(), Some(authorization));
        }
        assert_eq!(result.context().problems().len(), diagnostic_count);
        let definitions = result
            .context()
            .ownership_definitions(ObjectKind::SCHEMA, &FullName::from_local_name("app"));
        assert_eq!(definitions[0].owner(), Some(owner));
        Ok(())
    }
}

#[cfg(test)]
mod test_parameter_analysis_lifecycle {
    use crate::{
        ast::Dialect,
        context::ParseContext,
        parse_statements,
        sem::{
            SemAst, parameter_resolution::inference_runs as parameter_resolution_inference_runs,
        },
        test_helpers::{
            create_cache_context,
            public_analysis::{analyze, context},
            schema_layer, source_span,
        },
    };

    #[test]
    fn schema_loading_skips_parameter_inference() {
        use crate::{context::MutSourcePathWithSpan, names::SchemaName};
        let mut cache_context = create_cache_context();
        let before = parameter_resolution_inference_runs();
        let mut context = schema_layer(std::path::Path::new("schema.sql"), &context().freeze());
        context.set_source_path_with_span(&source_span());
        context.set_search_path(&[SchemaName::from("public"), SchemaName::from("pg_catalog")]);
        let statements = parse_statements(
            "create table t (n int4 default 1); create view v as select n from t group by n having true window w as (order by n) order by n limit 1 offset 0;",
            Dialect::postgres(),
        );
        for statement in statements {
            let statement = statement.unwrap();
            context.set_statement_span(*statement.span());
            context = SemAst::from_ast(&statement, context, &mut cache_context)
                .unwrap()
                .into_context();
        }
        assert_eq!(crate::sem::parameter_resolution::inference_runs(), before);
        assert!(context.parameter_constraints().result_column(0).is_none());
    }

    #[rstest::rstest]
    #[case::cast("select $1::missing")]
    #[case::function("select missing_function($1)")]
    #[case::operator("select $1 @@@ 1")]
    fn unresolved_parameter_operations_report_once(#[case] sql: &str) {
        let result = analyze(sql, Dialect::postgres())
            .expect("analysis recovers from unresolved operations");
        assert_eq!(result.context().reported_problem_count(), 1);
        assert!(
            result
                .context()
                .parameter_constraints()
                .parameter(1)
                .is_some()
        );
    }
}

#[cfg(test)]
mod test_structural_ast_analysis {
    use crate::{
        ast::{CteDefinition, Dialect, HaveWithClause},
        sem::{PgBuiltInType, SemAst},
        test_helpers::{TestParseContext, create_cache_context, public_analysis::statement},
    };

    #[rstest::rstest]
    #[case::postgres(Dialect::postgres())]
    #[cfg_attr(feature = "sqlite", case::sqlite(Dialect::sqlite()))]
    fn analyzes_structurally_inserted_dependencies(#[case] dialect: Dialect) {
        let mut cache_context = create_cache_context();
        let mut base = statement(
            "with existing as (select id from leaf) select id from existing",
            dialect,
        );
        let first = CteDefinition::new("root", &statement("select 1 as id", dialect)).unwrap();
        let second =
            CteDefinition::new("leaf", &statement("select id from root", dialect)).unwrap();
        let mut with = base.ensure_with_clause().unwrap();
        with.insert(0, &first).unwrap();
        with.insert(1, &second).unwrap();
        let mut context = TestParseContext::default().with_dialect(dialect);
        context.setup_type(PgBuiltInType::int4());
        let analyzed = SemAst::from_ast(&base, context, &mut cache_context).unwrap();
        let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
        let column = query.result_columns().get_column_at(0).unwrap();
        assert_eq!(column.col_type(), &Some(PgBuiltInType::int4()));
        assert_eq!(column.is_not_null(), &Some(true));
    }
}

#[cfg(test)]
mod test_rule_lifecycle {
    use crate::{
        context::{BaseParseContext, ParseContext, RuleInfoRead, RuleInfoWrite},
        names::{FullName, TableName as NamesTableName},
        sem::RuleIdentity,
        test_helpers::{rule as test_helpers, statements},
    };

    #[rstest::rstest]
    #[case::select_into("CREATE RULE r AS ON UPDATE TO items DO SELECT OLD.id INTO created")]
    #[case::missing_target("CREATE RULE r AS ON INSERT TO absent DO NOTHING")]
    #[case::bad_column("CREATE RULE r AS ON UPDATE TO items DO SELECT NEW.absent")]
    #[case::bad_old("CREATE RULE r AS ON INSERT TO items DO SELECT OLD.id")]
    #[case::bad_new("CREATE RULE r AS ON DELETE TO items DO SELECT NEW.id")]
    #[case::unqualified_action("CREATE RULE r AS ON UPDATE TO items DO SELECT id")]
    #[case::ambiguous_predicate(
        "CREATE RULE r AS ON UPDATE TO items WHERE id IS NOT NULL DO NOTHING"
    )]
    #[case::wrong_predicate_type("CREATE RULE r AS ON UPDATE TO items WHERE 1 DO NOTHING")]
    #[case::bind_parameter("CREATE RULE r AS ON UPDATE TO items DO SELECT $1")]
    #[case::where_bind_parameter("CREATE RULE r AS ON UPDATE TO items WHERE $1 DO NOTHING")]
    #[case::reserved_name(r#"CREATE RULE "_RETURN" AS ON INSERT TO items DO NOTHING"#)]
    #[case::select_table(
        r#"CREATE RULE "_RETURN" AS ON SELECT TO items DO INSTEAD SELECT id FROM sink"#
    )]
    #[case::select_name(
        "CREATE OR REPLACE RULE r AS ON SELECT TO item_view DO INSTEAD SELECT id FROM sink"
    )]
    #[case::select_nothing(
        r#"CREATE OR REPLACE RULE "_RETURN" AS ON SELECT TO item_view DO INSTEAD NOTHING"#
    )]
    #[case::select_also(
        r#"CREATE OR REPLACE RULE "_RETURN" AS ON SELECT TO item_view DO SELECT id FROM sink"#
    )]
    #[case::select_conditional(r#"CREATE OR REPLACE RULE "_RETURN" AS ON SELECT TO item_view WHERE true DO INSTEAD SELECT id FROM sink"#)]
    #[case::select_multiple(r#"CREATE OR REPLACE RULE "_RETURN" AS ON SELECT TO item_view DO INSTEAD (SELECT id FROM sink; SELECT id FROM sink)"#)]
    #[case::select_columns(r#"CREATE OR REPLACE RULE "_RETURN" AS ON SELECT TO item_view DO INSTEAD SELECT id AS different FROM sink"#)]
    #[case::select_modifying_cte(r#"CREATE OR REPLACE RULE "_RETURN" AS ON SELECT TO item_view DO INSTEAD WITH x AS (DELETE FROM sink RETURNING id) SELECT id FROM x"#)]
    #[case::select_old(
        r#"CREATE OR REPLACE RULE "_RETURN" AS ON SELECT TO item_view DO INSTEAD SELECT OLD.id"#
    )]
    #[case::returning_also(
        "CREATE RULE r AS ON INSERT TO items DO INSERT INTO sink VALUES(NEW.id) RETURNING id"
    )]
    #[case::returning_conditional(
        "CREATE RULE r AS ON INSERT TO items WHERE NEW.id IS NOT NULL DO INSTEAD INSERT INTO sink VALUES(NEW.id) RETURNING id"
    )]
    #[case::returning_multiple(
        "CREATE RULE r AS ON INSERT TO items DO INSTEAD (INSERT INTO sink VALUES(NEW.id) RETURNING id; INSERT INTO sink VALUES(NEW.id) RETURNING id)"
    )]
    #[case::returning_width(
        "CREATE RULE r AS ON INSERT TO items DO INSTEAD INSERT INTO sink VALUES(NEW.id) RETURNING id,id"
    )]
    #[case::cte_rows(
        "CREATE RULE r AS ON UPDATE TO items DO WITH a AS (SELECT OLD.id) SELECT * FROM a"
    )]
    #[case::cte_namespace(
        "CREATE RULE r AS ON UPDATE TO items DO WITH old AS (SELECT id FROM sink) SELECT id FROM old"
    )]
    #[case::exists_pending(
        "CREATE RULE r AS ON UPDATE TO items WHERE EXISTS(SELECT id FROM sink) DO NOTHING"
    )]
    #[case::any_pending(
        "CREATE RULE r AS ON UPDATE TO items DO SELECT OLD.id = ANY(SELECT id FROM sink)"
    )]
    #[case::source_namespace("CREATE RULE r AS ON UPDATE TO items DO SELECT old.id FROM sink old")]
    #[case::insert_union_rows(
        "CREATE RULE r AS ON INSERT TO items DO INSERT INTO sink SELECT NEW.id UNION SELECT NEW.id"
    )]
    #[case::insert_source_namespace(
        "CREATE RULE r AS ON INSERT TO items DO INSERT INTO sink SELECT old.id FROM sink old"
    )]
    #[case::union_rows("CREATE RULE r AS ON UPDATE TO items DO SELECT OLD.id UNION SELECT NEW.id")]
    fn rejects_invalid_definition_without_registration(#[case] sql: &str) {
        let mut context = test_helpers::catalog();
        assert!(
            statements::resolve_postgres(sql, &mut context)
                .is_ok_and(|ast| matches!(ast, crate::sem::SemAst::UnresolvedStatement)),
            "{sql}"
        );
        let identity = RuleIdentity::new(&FullName::with_schema("public", "items"), "r");
        assert!(context.rule(&identity).is_none());
        let mut has_rule_problem = false;
        for problem in context.reported_problems() {
            has_rule_problem |= problem.code() == "rule_invalid_definition";
        }
        assert!(has_rule_problem);
        assert!(
            context
                .get_table(&crate::names::TableName::from((identity.target()).clone()))
                .unwrap()
                .rules()
                .identities()
                .is_empty()
        );
    }

    #[test]
    fn named_lookup_target_links_and_layered_tombstones_agree() -> testresult::TestResult {
        let root = test_helpers::catalog().freeze();
        let mut context = test_helpers::layer(&root);
        for sql in [
            "CREATE RULE r AS ON UPDATE TO items DO SELECT NEW.id",
            "CREATE RULE s AS ON INSERT TO items DO INSTEAD NOTHING",
            "CREATE RULE r AS ON UPDATE TO item_view DO SELECT OLD.id",
        ] {
            context = test_helpers::apply(sql, context)?;
        }
        let identity = RuleIdentity::new(&FullName::with_schema("public", "items"), "r");
        let view_identity = RuleIdentity::new(&FullName::with_schema("public", "item_view"), "r");
        assert!(context.rule(&identity).is_some());
        assert!(context.rule(&view_identity).is_some());
        assert_eq!(
            context
                .get_table(&crate::names::TableName::from((identity.target()).clone()))
                .unwrap()
                .rules()
                .resolve(&context)
                .len(),
            2
        );
        assert_eq!(
            context
                .get_view(&crate::names::ViewName::from_full_name(
                    view_identity.target()
                ))
                .unwrap()
                .rules()
                .unwrap()
                .resolve(&context)
                .len(),
            1
        );
        let parent = context.freeze();
        let sibling = test_helpers::layer(&parent);
        let child = test_helpers::layer(&parent)
            .cancel_rule(&identity)?
            .dissociate_rule(identity.target(), &identity)?;
        assert!(child.rule(&identity).is_none());
        assert_eq!(
            child
                .get_table(&crate::names::TableName::from((identity.target()).clone()))
                .unwrap()
                .rules()
                .resolve(&child)
                .len(),
            1
        );
        assert!(parent.rule(&identity).is_some());
        assert_eq!(
            parent
                .get_table(&crate::names::TableName::from((identity.target()).clone()))
                .unwrap()
                .rules()
                .resolve(&parent)
                .len(),
            2
        );
        assert!(sibling.rule(&identity).is_some());
        assert!(child.rule(&view_identity).is_some());
        let descendant = test_helpers::layer(&child.freeze());
        assert!(descendant.rule(&identity).is_none());
        let recreated = test_helpers::apply(
            "CREATE RULE r AS ON UPDATE TO items DO INSTEAD NOTHING",
            descendant,
        )?;
        assert!(recreated.rule(&identity).unwrap().instead());
        assert_eq!(
            recreated
                .get_table(&crate::names::TableName::from((identity.target()).clone()))
                .unwrap()
                .rules()
                .resolve(&recreated)
                .len(),
            2
        );
        Ok(())
    }

    #[test]
    fn exact_identities_preserve_relation_scope_and_quoted_components() -> testresult::TestResult {
        let mut catalog = test_helpers::catalog();
        for name in [
            FullName::with_schema("public", "a.b"),
            FullName::with_schema("public.a", "b"),
        ] {
            test_helpers::add_table(&mut catalog, &name);
        }
        let root = catalog.freeze();
        let mut context = test_helpers::layer(&root);
        for sql in [
            "CREATE RULE r AS ON INSERT TO items DO NOTHING",
            "CREATE RULE r AS ON INSERT TO sink DO NOTHING",
            r#"CREATE RULE "R.x" AS ON INSERT TO public."a.b" DO NOTHING"#,
            r#"CREATE RULE "R.x" AS ON INSERT TO "public.a".b DO NOTHING"#,
        ] {
            context = test_helpers::apply(sql, context)?;
        }
        for (target, name) in [
            (FullName::with_schema("public", "items"), "r"),
            (FullName::with_schema("public", "sink"), "r"),
            (FullName::with_schema("public", "a.b"), "R.x"),
            (FullName::with_schema("public.a", "b"), "R.x"),
        ] {
            let identity = RuleIdentity::new(&target, name);
            let registered = context.rule(&identity).unwrap();
            assert_eq!(registered.target(), &target);
            let linked = context
                .get_table(&NamesTableName::from(target.clone()))
                .unwrap()
                .rules()
                .resolve(&context);
            assert_eq!(linked.len(), 1);
            assert!(std::ptr::eq(linked[0], registered));
        }
        let unresolved = statements::resolve_postgres(
            "CREATE RULE r AS ON INSERT TO items DO NOTHING",
            &mut context,
        )
        .unwrap();
        assert!(matches!(
            unresolved,
            crate::sem::SemAst::UnresolvedStatement
        ));
        assert!(
            context
                .problems()
                .iter()
                .any(|problem| problem.code() == "rule_invalid_definition")
        );
        Ok(())
    }
}

#[cfg(test)]
mod test_trigger_lifecycle {

    use crate::{
        ast::Dialect,
        context::{ParseContext, TableInfoRead, TriggerInfoRead},
        names::FullName,
        sem::TriggerIdentity,
        test_helpers::trigger as test_helpers,
    };

    #[rstest::rstest]
    #[case::set_returning_function(
        "CREATE TRIGGER audit AFTER UPDATE ON items EXECUTE FUNCTION set_trigger_fn()"
    )]
    #[case::aggregate_when(
        "CREATE TRIGGER audit BEFORE UPDATE ON items FOR EACH ROW WHEN (aggregate_fn()) EXECUTE FUNCTION audit_fn()"
    )]
    #[case::unknown_target(
        "CREATE TRIGGER audit BEFORE INSERT ON absent EXECUTE FUNCTION audit_fn()"
    )]
    #[case::unknown_column(
        "CREATE TRIGGER audit BEFORE UPDATE OF absent ON items EXECUTE FUNCTION audit_fn()"
    )]
    #[case::unknown_function(
        "CREATE TRIGGER audit BEFORE INSERT ON items EXECUTE FUNCTION absent()"
    )]
    #[case::old_insert(
        "CREATE TRIGGER audit BEFORE INSERT ON items FOR EACH ROW WHEN (OLD.id IS NULL) EXECUTE FUNCTION audit_fn()"
    )]
    #[case::new_delete(
        "CREATE TRIGGER audit BEFORE DELETE ON items FOR EACH ROW WHEN (NEW.id IS NULL) EXECUTE FUNCTION audit_fn()"
    )]
    #[case::mixed_old(
        "CREATE TRIGGER audit BEFORE INSERT OR UPDATE ON items FOR EACH ROW WHEN (OLD.id IS NULL) EXECUTE FUNCTION audit_fn()"
    )]
    #[case::unqualified(
        "CREATE TRIGGER audit BEFORE UPDATE ON items FOR EACH ROW WHEN (id IS NULL) EXECUTE FUNCTION audit_fn()"
    )]
    #[case::statement_old(
        "CREATE TRIGGER audit BEFORE UPDATE ON items WHEN (OLD.id IS NULL) EXECUTE FUNCTION audit_fn()"
    )]
    #[case::instead_table(
        "CREATE TRIGGER audit INSTEAD OF UPDATE ON items FOR EACH ROW EXECUTE FUNCTION audit_fn()"
    )]
    #[case::truncate_row(
        "CREATE TRIGGER audit AFTER TRUNCATE ON items FOR EACH ROW EXECUTE FUNCTION audit_fn()"
    )]
    #[case::transition_columns(
        "CREATE TRIGGER audit AFTER UPDATE OF id ON items REFERENCING OLD TABLE AS previous EXECUTE FUNCTION audit_fn()"
    )]
    #[case::transition_event(
        "CREATE TRIGGER audit AFTER INSERT ON items REFERENCING OLD TABLE AS previous EXECUTE FUNCTION audit_fn()"
    )]
    #[case::not_boolean(
        "CREATE TRIGGER audit BEFORE UPDATE ON items FOR EACH ROW WHEN (NEW.id) EXECUTE FUNCTION audit_fn()"
    )]
    #[case::subquery(
        "CREATE TRIGGER audit BEFORE UPDATE ON items FOR EACH ROW WHEN (EXISTS(SELECT 1)) EXECUTE FUNCTION audit_fn()"
    )]
    fn rejects_invalid_postgres_semantics(#[case] sql: &str) {
        let mut context = test_helpers::catalog(Dialect::postgres());
        assert!(
            test_helpers::resolve(sql, &mut context)
                .is_ok_and(|ast| matches!(ast, crate::sem::SemAst::UnresolvedStatement)),
            "{sql}"
        );
        assert!(context.reported_problem_count() > 0);
        let key = TriggerIdentity::postgres(
            &FullName::with_schema("public", "items"),
            &FullName::with_schema("public", "audit"),
        );
        assert!(context.trigger(&key).is_none());
    }

    #[test]
    fn failed_child_analysis_preserves_parent_and_sibling_catalogs() -> testresult::TestResult {
        let root = test_helpers::catalog(Dialect::postgres()).freeze();
        let target = FullName::with_schema("public", "items");
        let sibling = test_helpers::layer(&root);
        assert!(test_helpers::apply("CREATE TRIGGER audit BEFORE UPDATE ON items FOR EACH ROW WHEN (missing IS NULL) EXECUTE FUNCTION audit_fn()", test_helpers::layer(&root)).is_ok_and(|pair| matches!(pair.ast(), crate::sem::SemAst::UnresolvedStatement) && !pair.context().problems().is_empty()));
        assert!(
            root.get_table_impl(&target)
                .unwrap()
                .triggers()
                .names()
                .is_empty()
        );
        assert!(
            sibling
                .get_table_impl(&target)
                .unwrap()
                .triggers()
                .names()
                .is_empty()
        );
        assert!(
            test_helpers::apply(
                "CREATE TRIGGER audit AFTER UPDATE ON items EXECUTE FUNCTION audit_fn()",
                sibling
            )
            .is_ok()
        );
        Ok(())
    }

    #[cfg(feature = "sqlite")]
    #[rstest::rstest]
    #[case::parameter(
        "CREATE TRIGGER audit AFTER UPDATE ON items WHEN ?1 BEGIN SELECT NEW.id; END"
    )]
    #[case::cross_database(
        "CREATE TRIGGER audit AFTER UPDATE ON items BEGIN SELECT id FROM aux.other; END"
    )]
    fn rejects_invalid_sqlite_definition(#[case] sql: &str) -> testresult::TestResult {
        let oracle = test_helpers::sqlite_oracle()?;
        oracle.execute_batch("ATTACH ':memory:' AS aux; CREATE TABLE aux.other(id INTEGER);")?;
        assert!(oracle.execute_batch(sql).is_err());
        let mut context = test_helpers::catalog(Dialect::sqlite());
        test_helpers::add_tables(&mut context, "aux");
        assert!(
            test_helpers::resolve(sql, &mut context)
                .is_ok_and(|ast| matches!(ast, crate::sem::SemAst::UnresolvedStatement))
        );
        Ok(())
    }
}

#[cfg(test)]
mod test_analyze_constraint {
    use crate::test_helpers::{
        constr_attr_deferrable, constr_attr_deferred, constr_attr_immediate,
        constr_attr_not_deferrable, constr_check, constr_default, constr_exclusion, constr_foreign,
        constr_generated, constr_identity, constr_not_null, constr_null, constr_primary,
        constr_unique,
    };

    use crate::{
        ast::{ColumnConstraint as AstColumnConstraint, TryFromWithTokens},
        names::{ColumnName, FullName, TableName},
        sem::{ColumnConstraint, ColumnConstraintKind, ColumnConstraintTrait},
        syn::Constraint,
        test_helpers::{TestParseContext, test_context},
    };

    use testresult::TestResult;

    #[rstest::rstest]
    #[case::constr_not_null(constr_not_null(), ColumnConstraintKind::Nullability)]
    #[case::constr_null(constr_null(), ColumnConstraintKind::Nullability)]
    #[case::constr_default(constr_default(), ColumnConstraintKind::Default)]
    #[case::constr_identity(constr_identity(), ColumnConstraintKind::Identity)]
    #[case::constr_generated(constr_generated(), ColumnConstraintKind::Generated)]
    #[case::constr_check(constr_check(), ColumnConstraintKind::Check)]
    #[case::constr_primary(constr_primary(), ColumnConstraintKind::PrimaryKey)]
    #[case::constr_unique(constr_unique(), ColumnConstraintKind::Unique)]
    #[case::constr_exclusion(constr_exclusion(), ColumnConstraintKind::Exclusion)]
    #[case::constr_foreign(constr_foreign(), ColumnConstraintKind::ForeignKey)]
    #[case::constr_deferrable(constr_attr_deferrable(), ColumnConstraintKind::AttrDeferrability)]
    #[case::constr_deferrable(
        constr_attr_not_deferrable(),
        ColumnConstraintKind::AttrDeferrability
    )]
    #[case::constr_deferred(constr_attr_deferred(), ColumnConstraintKind::AttrDeferred)]
    #[case::constr_immediate(constr_attr_immediate(), ColumnConstraintKind::AttrImmediate)]
    fn test_analyze_constraint(
        test_context: TestParseContext,
        #[case] syn: Constraint,
        #[case] kind: ColumnConstraintKind,
    ) -> TestResult {
        let table_name = TableName::from(FullName::from_local_name("table1"));
        let column_name = ColumnName::new("col1");
        let tokens = Vec::default().into();
        let syntax = AstColumnConstraint::try_from_with_tokens(&syn, &tokens)?;
        let (constraint, _new_context) = ColumnConstraint::from_ast_and_report(
            &syntax,
            test_context,
            &table_name,
            &column_name,
        )?;
        assert_eq!(constraint.kind(), kind);
        Ok(())
    }
}

#[cfg(test)]
mod test_invalid_drop_leaves_named_definitions_and_links_intact {
    use crate::{
        context::{ParseContext, RuleInfoRead},
        sem::SemAst,
        test_helpers::{
            create_cache_context, drop_rule as test_helpers, heavy::drop_rule as heavy,
        },
    };

    #[rstest::rstest]
    #[case::missing_rule("DROP RULE missing ON item_view")]
    #[case::missing_relation("DROP RULE audit ON absent")]
    fn invalid_drop_leaves_named_definitions_and_links_intact(
        #[case] sql: &str,
    ) -> testresult::TestResult {
        let parent = heavy::catalog()?;
        let mut child = test_helpers::layer(&parent);
        assert!(
            SemAst::from_ast_node(
                test_helpers::statement(sql).node(),
                &mut child,
                &mut create_cache_context()
            )
            .is_ok_and(|ast| matches!(ast, crate::sem::SemAst::UnresolvedStatement))
        );
        assert!(child.rule(&test_helpers::identity()).is_some());
        assert_eq!(test_helpers::active_count(&child.freeze()), 2);
        Ok(())
    }
}

#[cfg(test)]
mod test_postgres_scalar_syntax {
    use crate::{
        sem::{
            ScalarAnalysisContext,
            ScalarConstExpr::*,
            SemScalarExpr::{self, *},
        },
        test_helpers::{TestParseContext, ast_query, test_context},
    };

    use ::testresult::TestResult;

    #[rstest::rstest]
    #[case("42", Const(Integer(42)))]
    #[case("null", Const(Null))]
    fn test_analyze_scalar_expr(
        test_context: TestParseContext,
        #[case] sql_expr: &str,
        #[case] expect_expr: SemScalarExpr,
    ) -> TestResult {
        let syntax = ast_query::postgres_expression(sql_expr)?;
        let mut context = test_context;
        let expr =
            SemScalarExpr::from_ast(&syntax, &mut context, &ScalarAnalysisContext::default())?;
        assert_eq!(expr, expect_expr);

        Ok(())
    }
}

#[cfg(all(test, feature = "sqlite"))]
#[cfg(test)]
mod test_sqlite_scalar_syntax {
    use crate::{
        ast::Dialect,
        sem::{
            ArithExpr, ScalarAnalysisContext,
            ScalarConstExpr::*,
            SemScalarExpr::{self, *},
        },
        test_helpers::{TestParseContext, ast_query, test_context},
    };

    use ::testresult::TestResult;

    #[rstest::rstest]
    #[case("42", Const(Integer(42)))]
    #[case("null", Const(Null))]
    #[case("10 between 1 and 100", ArithExpr::between(
            Const(Integer(10)), (Const(Integer(1)), Const(Integer(100)))
        ))]
    #[case("10 not between 1 and 100", ArithExpr::not_between(
            Const(Integer(10)), (Const(Integer(1)), Const(Integer(100)))
        ))]
    fn test_analyze_sqlite_scalar_expr(
        test_context: TestParseContext,
        #[case] sql_expr: &str,
        #[case] expect_expr: SemScalarExpr,
    ) -> TestResult {
        let syntax = ast_query::sqlite_expression(sql_expr)?;
        let mut context = test_context.with_dialect(Dialect::sqlite());
        let sem_expr =
            SemScalarExpr::from_ast(&syntax, &mut context, &ScalarAnalysisContext::default())?;
        assert_eq!(sem_expr, expect_expr);
        Ok(())
    }
}

#[cfg(test)]
mod test_rejected_create_retains_analysis_without_publishing_table {
    use crate::{
        context::TableInfoRead,
        test_helpers::{foreign_keys as fk, ownership::apply_ownership_sql},
    };

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

#[cfg(test)]
mod test_table_output_validation {
    use crate::test_helpers::query::table_function_query;

    #[rstest::rstest]
    #[case::missing("SELECT 1 FROM record_fun('') r", "table_function_definition_required")]
    #[case::duplicate(
        "SELECT 1 FROM record_fun('') r(a int,a text)",
        "table_function_duplicate_column"
    )]
    #[case::unknown_type(
        "SELECT 1 FROM record_fun('') r(a missing)",
        "table_function_type_not_found"
    )]
    #[case::scalar(
        "SELECT 1 FROM identity(1) r(a int)",
        "table_function_definition_not_allowed"
    )]
    #[case::composite(
        "SELECT 1 FROM pair_fun() r(a int)",
        "table_function_definition_not_allowed"
    )]
    #[case::out(
        "SELECT 1 FROM out_fun() r(a int)",
        "table_function_definition_not_allowed"
    )]
    #[case::too_many_aliases(
        "SELECT 1 FROM pair_fun() r(a,b,c)",
        "table_function_too_many_aliases"
    )]
    #[case::unknown_collation(
        r#"SELECT 1 FROM record_fun('') r(a text COLLATE missing)"#,
        "collation_not_found"
    )]
    #[case::named_missing(
        "SELECT 1 FROM record_fun(src => '') r",
        "table_function_definition_required"
    )]
    #[case::rows_missing(
        "SELECT 1 FROM ROWS FROM(record_fun('')) r",
        "table_function_definition_required"
    )]
    #[case::rows_duplicate(
        "SELECT 1 FROM ROWS FROM(record_fun('') AS(a int,a text)) r",
        "table_function_duplicate_column"
    )]
    #[case::rows_multiple_outer(
        "SELECT 1 FROM ROWS FROM(record_fun(''),record_fun('')) r(a int)",
        "table_function_definition_placement"
    )]
    #[case::rows_conflicting(
        "SELECT 1 FROM ROWS FROM(record_fun('') AS(a int)) r(b int)",
        "table_function_definition_placement"
    )]
    #[case::rows_outer_ordinality(
        "SELECT 1 FROM ROWS FROM(record_fun('')) WITH ORDINALITY r(a int)",
        "table_function_definition_placement"
    )]
    #[case::ordinary_ordinality(
        "SELECT 1 FROM record_fun('') WITH ORDINALITY r(a int)",
        "table_function_definition_placement"
    )]
    fn diagnoses_invalid_definitions(#[case] sql: &str, #[case] code: &str) {
        let (_, context) = table_function_query(sql, false);
        assert!(
            context.has_reported_problem(code),
            "{sql}: {:?}",
            context.reported_problems()
        );
    }

    #[rstest::rstest]
    #[case::missing("SELECT 1 FROM record_fun('') r")]
    #[case::duplicate("SELECT 1 FROM record_fun('') r(a int,a text)")]
    #[case::unknown_type("SELECT 1 FROM record_fun('') r(a missing)")]
    #[case::deferred_missing("SELECT 1 FROM record_fun(src => '') r")]
    fn retains_invalid_definition_state_after_serialization(#[case] sql: &str) {
        use crate::{names::AliasName, sem::DataSource};
        let (query, context) = table_function_query(sql, false);
        assert!(context.reported_problem_count() > 0);
        let source = query
            .query_clauses()
            .unwrap()
            .body()
            .source_scope()
            .unwrap()
            .resolve_alias(&AliasName::from("r"))
            .unwrap()
            .0;
        let restored: DataSource =
            ::serde_json::from_str(&::serde_json::to_string(source).unwrap()).unwrap();
        assert_eq!(&restored, source);
        let problems = if let Some(deferred) = restored.deferred_function() {
            deferred.output_problems()
        } else if let DataSource::CallTableFunc(function) = &restored {
            function.output_problems()
        } else {
            panic!("function invocation");
        };
        assert!(!problems.is_empty());
    }
}

#[cfg(test)]
mod test_grouping_validation {
    use crate::{
        ast::Dialect,
        test_helpers::{grouping::groupings, query::resolve},
    };

    #[rstest::rstest]
    #[case::mismatch(
        "SELECT GROUPING(a) FROM (SELECT 1 AS a) s GROUP BY a+1",
        "query_grouping_argument_not_grouped"
    )]
    #[case::function_missing(
        "SELECT GROUPING(missing(a)) FROM (SELECT 1 AS a) s GROUP BY missing(a)",
        "query_grouping_argument_not_grouped"
    )]
    #[case::both_missing(
        "SELECT GROUPING(missing) GROUP BY missing",
        "query_grouping_argument_not_grouped"
    )]
    #[case::missing(
        "SELECT GROUPING(missing) FROM (SELECT 1 AS a) s GROUP BY a",
        "query_grouping_argument_not_grouped"
    )]
    #[case::ambiguous(
        "SELECT GROUPING(a) FROM (SELECT 1 AS a) s, (SELECT 2 AS a) t GROUP BY s.a",
        "query_grouping_argument_not_grouped"
    )]
    #[case::where_clause(
        "SELECT a FROM (SELECT 1 AS a) s WHERE GROUPING(a) IS NULL GROUP BY a",
        "query_invalid_grouping_placement"
    )]
    #[case::group_clause(
        "SELECT 1 FROM (SELECT 1 AS a) s GROUP BY GROUPING(a)",
        "query_invalid_grouping_placement"
    )]
    #[case::group_alias(
        "SELECT GROUPING(a) AS g FROM (SELECT 1 AS a) s GROUP BY g,a",
        "query_invalid_grouping_placement"
    )]
    #[case::group_ordinal(
        "SELECT GROUPING(a) FROM (SELECT 1 AS a) s GROUP BY 1,a",
        "query_invalid_grouping_placement"
    )]
    #[case::group_wrapped_alias(
        "SELECT GROUPING(a)+1 AS g FROM (SELECT 1 AS a) s GROUP BY ROLLUP(g,a)",
        "query_invalid_grouping_placement"
    )]
    #[case::group_correlated_alias(
        "SELECT (SELECT GROUPING(s.a)) AS g FROM (SELECT 1 AS a) s GROUP BY g,a",
        "query_invalid_grouping_placement"
    )]
    #[case::nested(
        "SELECT GROUPING(GROUPING(a)) FROM (SELECT 1 AS a) s GROUP BY a",
        "query_invalid_grouping_placement"
    )]
    #[case::aggregate_argument(
        "SELECT sum(GROUPING(a)) FROM (SELECT 1 AS a) s GROUP BY a",
        "query_invalid_grouping_placement"
    )]
    #[case::correlated_aggregate_inside(
        "SELECT GROUPING((SELECT sum(s.a))) FROM (SELECT 1 AS a) s GROUP BY (SELECT sum(s.a))",
        "query_invalid_grouping_placement"
    )]
    #[case::aggregate_inside(
        "SELECT GROUPING(sum(a)) FROM (SELECT 1 AS a) s GROUP BY a",
        "query_invalid_grouping_placement"
    )]
    #[case::filter(
        "SELECT sum(a) FILTER (WHERE GROUPING(a) IS NULL) FROM (SELECT 1 AS a) s GROUP BY a",
        "query_invalid_grouping_placement"
    )]
    #[case::frame(
        "SELECT sum(a) OVER (ROWS GROUPING(a) PRECEDING) FROM (SELECT 1 AS a) s GROUP BY a",
        "query_invalid_grouping_placement"
    )]
    #[case::row_limit("SELECT 1 LIMIT GROUPING(1)", "query_invalid_grouping_placement")]
    #[case::no_grouping("SELECT GROUPING(1)", "query_grouping_argument_not_grouped")]
    #[case::inner_constant(
        "SELECT (SELECT GROUPING(1)) FROM (SELECT 1 AS a) s GROUP BY a",
        "query_grouping_argument_not_grouped"
    )]
    #[case::correlated_where(
        "SELECT 1 FROM (SELECT 1 AS a) s WHERE (SELECT GROUPING(s.a)) IS NULL GROUP BY a",
        "query_invalid_grouping_placement"
    )]
    #[case::correlated_aggregate(
        "SELECT sum((SELECT GROUPING(s.a))) FROM (SELECT 1 AS a) s GROUP BY a",
        "query_invalid_grouping_placement"
    )]
    fn reports_grouping_errors(#[case] sql: &str, #[case] code: &str) {
        let (query, context) = resolve(sql, Dialect::postgres(), false).unwrap();
        assert!(
            context
                .reported_problems()
                .iter()
                .any(|problem| problem.code() == code),
            "{sql}: {:?}",
            context.reported_problems()
        );
        let groupings = groupings(&query);
        assert!(!groupings.is_empty());
        for grouping in groupings {
            assert!(!grouping.is_valid(), "{sql}: {grouping:?}");
        }
    }
}

#[cfg(test)]
mod test_grouping_lateral_scope {
    use crate::{
        ast::Dialect,
        test_helpers::{grouping::groupings, query::resolve},
    };

    #[rstest::rstest]
    #[case::correlated_from(
        "SELECT * FROM (VALUES (1)) s(a), LATERAL (SELECT GROUPING(s.a) GROUP BY s.a) x",
        false,
        1
    )]
    #[case::correlated_join(
        "SELECT * FROM (VALUES (1)) s(a) JOIN LATERAL (SELECT GROUPING(s.a) GROUP BY s.a) x ON true",
        false,
        1
    )]
    #[case::local(
        "SELECT * FROM (VALUES (1)) s(a), LATERAL (SELECT GROUPING(t.a) FROM (VALUES(2)) t(a) GROUP BY t.a) x",
        true,
        0
    )]
    #[case::mixed(
        "SELECT * FROM (VALUES (1)) s(a), LATERAL (SELECT GROUPING(t.a+s.a) FROM (VALUES(2)) t(a) GROUP BY t.a+s.a) x",
        true,
        0
    )]
    fn binds_grouping_in_lateral_sources(
        #[case] sql: &str,
        #[case] valid: bool,
        #[case] levels_up: usize,
        #[values(false, true)] infer: bool,
    ) {
        let (query, context) = resolve(sql, Dialect::postgres(), infer).unwrap();
        let groupings = groupings(&query);
        assert!(!groupings.is_empty());
        for grouping in groupings {
            assert_eq!(grouping.is_valid(), valid, "{sql}: {grouping:?}");
            assert_eq!(grouping.levels_up(), levels_up, "{sql}: {grouping:?}");
            assert_eq!(
                grouping.arguments()[0].key_index(),
                if valid { Some(0) } else { None }
            );
        }
        if valid {
            assert_eq!(
                context.reported_problem_count(),
                0,
                "{sql}: {:?}",
                context.reported_problems()
            );
        } else {
            assert!(
                context
                    .reported_problems()
                    .iter()
                    .any(|problem| problem.code() == "query_invalid_grouping_placement"),
                "{sql}: {:?}",
                context.reported_problems()
            );
        }
    }
}
