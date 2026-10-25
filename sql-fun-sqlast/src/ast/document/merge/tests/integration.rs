use super::test_helpers::catalog;
use crate::ast::{CteDefinition, Dialect, HaveWithClause, Statement, StatementKind};
use crate::context::ParseContext;
use crate::sem::{ColumnName, PgBuiltInType};

fn statement(sql: &str) -> Statement {
    crate::parse_statements(sql, Dialect::postgres())
        .next()
        .expect("one statement")
        .expect(sql)
}

#[test]
fn public_statement_entry_point_analyzes_and_deparses_merge() {
    let source = statement(
        "MERGE INTO items AS t USING incoming AS s ON true WHEN MATCHED THEN UPDATE SET label = s.slabel RETURNING t.id",
    );
    assert!(source.is_merge());
    assert_eq!(source.kind(), StatementKind::merge());

    let analyzed = source.analyze(catalog(17)).expect("MERGE analysis");
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
fn merge_cte_alias_is_visible_to_the_outer_query() {
    let source = statement(
        "WITH changed(renamed_id) AS (MERGE INTO items AS t USING incoming AS s ON true WHEN MATCHED THEN UPDATE SET label = s.slabel RETURNING t.id) SELECT renamed_id FROM changed",
    );
    let analyzed = source.analyze(catalog(17)).expect("MERGE CTE analysis");
    let query = analyzed
        .ast()
        .clone()
        .into_select_statemtnt()
        .expect("SELECT result");
    let column = query
        .get_result_column_def(&ColumnName::from("renamed_id"))
        .expect("aliased MERGE output");
    assert_eq!(column.get_type(), Some(&PgBuiltInType::int4()));
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
    assert!(reparsed.analyze(catalog(17)).is_ok());
}

#[test]
fn nested_merge_cte_placement_is_rejected() {
    let source = statement(
        "SELECT * FROM (WITH changed AS (MERGE INTO items AS t USING incoming AS s ON true WHEN MATCHED THEN UPDATE SET label = s.slabel RETURNING t.id) SELECT id FROM changed) AS nested",
    );
    let result = source.analyze(catalog(17));
    assert!(match result {
        Err(_) => true,
        Ok(analyzed) => analyzed.context().reported_problem_count() > 0,
    });
}

#[test]
fn returning_parameter_defaults_to_text_before_merge_action_assignment() {
    let source = statement(
        "MERGE INTO items AS t USING incoming AS s ON t.id = $2 WHEN MATCHED AND $3 THEN UPDATE SET id = $1 WHEN NOT MATCHED THEN INSERT (id) VALUES ($1) RETURNING $1 AS value",
    );
    let result = source.analyze(catalog(17));
    assert!(match result {
        Err(_) => true,
        Ok(analyzed) => analyzed.context().reported_problem_count() > 0,
    });
}

#[test]
fn postgres_18_merge_row_image_stars_expand_the_target_columns() {
    let source = statement(
        "MERGE INTO items AS t USING incoming AS s ON true WHEN MATCHED THEN UPDATE SET label = s.slabel WHEN NOT MATCHED BY SOURCE THEN DELETE WHEN NOT MATCHED BY TARGET THEN INSERT (id, label) VALUES (s.sid, s.slabel) RETURNING old.*, new.*",
    );
    let analyzed = source
        .analyze(catalog(18))
        .expect("MERGE row-image analysis");
    assert_eq!(analyzed.ast().as_merge().unwrap().result_columns().len(), 4);
}

#[test]
fn cte_source_and_merge_actions_share_parameter_analysis() {
    let source = statement(
        "WITH input AS (SELECT $1::integer AS sid) MERGE INTO items t USING input s ON true WHEN MATCHED THEN UPDATE SET id=s.sid WHEN NOT MATCHED THEN INSERT(id) VALUES(s.sid) RETURNING t.id",
    );
    let before = crate::sem::parameter_resolution::inference_runs();
    let analyzed = source.analyze(catalog(17)).unwrap();
    assert_eq!(analyzed.context().reported_problem_count(), 0);
    assert_eq!(
        crate::sem::parameter_resolution::inference_runs(),
        before + 1
    );
    assert!(
        analyzed
            .context()
            .parameter_constraints()
            .parameter(1)
            .unwrap()
            .constraint()
            .allows(&PgBuiltInType::int4())
    );
}
