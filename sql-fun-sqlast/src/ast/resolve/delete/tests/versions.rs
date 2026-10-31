use crate::ast::Dialect;

#[rstest::rstest]
#[case::postgres17("17", true)]
#[case::postgres18("18", false)]
#[case::custom("custom-engine", true)]
fn returning_row_images_use_configured_engine_version(
    #[case] version: &str,
    #[case] missing: bool,
) {
    let mut context = test_helpers::catalog_for_version(version);
    let statement = super::test_helpers::resolve(
        "DELETE FROM items RETURNING old.id,new.id",
        &mut context,
        Dialect::postgres(),
    )
    .unwrap();
    assert_eq!(context.reported_problem_count() > 0, missing);
    if !missing {
        let columns = statement.as_delete().unwrap().result_columns();
        assert_eq!(columns.get_column_at(0).unwrap().is_not_null(), &Some(true));
        assert_eq!(
            columns.get_column_at(1).unwrap().is_not_null(),
            &Some(false)
        );
    }
}

#[rstest::rstest]
#[case::old_star("DELETE FROM items RETURNING old.*", 2)]
#[case::new_star("DELETE FROM items RETURNING new.*", 2)]
#[case::nested("DELETE FROM items RETURNING (SELECT old.id)", 1)]
fn row_images_preserve_typed_catalog_bindings(#[case] sql: &str, #[case] width: usize) {
    let mut context = test_helpers::catalog_for_version("18");
    let result = super::test_helpers::resolve(sql, &mut context, Dialect::postgres()).unwrap();
    assert_eq!(
        context.reported_problem_count(),
        0,
        "{:?}",
        context.reported_problems()
    );
    assert_eq!(result.as_delete().unwrap().result_columns().len(), width);
    if sql.contains(".*") {
        let column = result
            .as_delete()
            .unwrap()
            .result_columns()
            .get_column_at(0)
            .unwrap();
        let binding = column.expression().unwrap().delete_row_reference().unwrap();
        assert!(binding.source().get_column_def().is_some());
        assert_eq!(
            binding.version(),
            if sql.contains("old.*") {
                crate::sem::DeleteRowVersion::old()
            } else {
                crate::sem::DeleteRowVersion::new_image()
            }
        );
    }
}

#[rstest::rstest]
fn explicit_old_alias_is_ordinary_source(#[values("17", "18")] version: &str) {
    let mut context = test_helpers::catalog_for_version(version);
    let result = super::test_helpers::resolve(
        "DELETE FROM items AS old RETURNING old.id",
        &mut context,
        Dialect::postgres(),
    )
    .unwrap();
    assert_eq!(context.reported_problem_count(), 0);
    assert!(
        result
            .as_delete()
            .unwrap()
            .result_columns()
            .get_column_at(0)
            .unwrap()
            .expression()
            .unwrap()
            .delete_row_reference()
            .is_none()
    );
}

mod test_helpers {
    pub(super) fn catalog_for_version(version: &str) -> crate::test_helpers::TestParseContext {
        let metadata = ::serde_json::from_value(::serde_json::json!({
            "engine": "postgres", "dialect": "postgres", "version": version, "search_path": ["public"]
        })).unwrap();
        super::super::test_helpers::catalog(crate::ast::Dialect::postgres())
            .with_metadata(&metadata)
    }
}
