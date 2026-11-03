use crate::ast::wildcard::rows_source::PrivateNames;
use crate::test_helpers::wildcard_rows_source::invocation;
use crate::test_helpers::wildcard_rows_source::layout;

#[test]
fn private_names_do_not_shadow_correlated_bindings_or_output_columns() {
    let source =
        invocation("SELECT * FROM ROWS FROM (generate_series(1,__sql_fun_rows_0.id)) AS r");
    let resolved = layout(&[&["__sql_fun_rows_1"]], &["__sql_fun_rows_2"], false);
    let mut names = PrivateNames::new(&source, &resolved).unwrap();
    assert_eq!(names.next(), "__sql_fun_rows_3");
    assert_eq!(names.next(), "__sql_fun_rows_4");
}
