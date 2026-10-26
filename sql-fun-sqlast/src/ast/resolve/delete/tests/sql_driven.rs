//! DELETE analysis through the public catalog and statement APIs.
use crate::context::BaseParseContext;
use crate::context::ParseContext;
use crate::{
    ast::Dialect,
    parse_statements,
    sem::{ColumnName, FullName, PgBuiltInType, TableName},
};

#[test]
fn sql_driven_delete_cte_preserves_bindings_types_and_schema()
-> Result<(), Box<dyn std::error::Error>> {
    let mut context = test_helpers::context();
    for statement in parse_statements(
        "CREATE TABLE items(id integer NOT NULL, label text); CREATE TABLE sources(id integer, label text)",
        Dialect::postgres(),
    ) {
        context = statement?.analyze(context)?.into_context();
    }
    let statement = parse_statements("WITH d(removed,label) AS (DELETE FROM items AS i USING sources AS s WHERE i.id=s.id AND i.id=$1 RETURNING i.id,s.label) SELECT removed,label FROM d", Dialect::postgres()).next().unwrap()?;
    let analyzed = statement.analyze(context)?;
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
    let crate::sem::SelectStatement::Query(boundary) = &query else {
        panic!("query boundary")
    };
    let crate::sem::CteStatement::Delete(delete) = boundary.with()[0].statement() else {
        panic!("DELETE CTE")
    };
    assert_eq!(delete.target().to_string(), "public.items");
    for (alias, relation) in [("i", "items"), ("s", "sources")] {
        let source = delete
            .from()
            .resolve_alias(&crate::sem::AliasName::from(alias))
            .unwrap()
            .0;
        let crate::sem::DataSource::Table(table) = source else {
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

mod test_helpers {
    use crate::sem::{MutSourcePathWithSpan, PgBuiltInType, SchemaName};
    use crate::test_helpers::{TestParseContext, source_span};

    pub(super) fn context() -> TestParseContext {
        let mut context = TestParseContext::default().with_dialect(crate::ast::Dialect::postgres());
        context.set_source_path_with_span(&source_span());
        context.set_get_search_path_result(&[
            SchemaName::from("public"),
            SchemaName::from("pg_catalog"),
        ]);
        context.setup_type(PgBuiltInType::int4());
        context.setup_type(PgBuiltInType::text());
        context.setup_type(PgBuiltInType::bool());
        context.setup_binary("=", "int4", "int4", "bool", true);
        context
    }
}
