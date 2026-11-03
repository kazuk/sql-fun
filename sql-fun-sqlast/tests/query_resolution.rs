//! Compatibility checks against the public, non-unit-test analysis boundary.

// Shared support lives below tests/ to avoid an extra Cargo test target.
#[path = "test_helpers/shared.rs"]
mod test_helpers;

use crate::test_helpers::heavy::query_resolution as heavy;

#[rstest::rstest]
#[case::literal("select 1 as n")]
#[case::row("select row(1, 2)")]
#[case::invalid_star_diagnostic("select (1).*")]
fn supported_select_reaches_public_resolution(#[case] sql: &str) {
    assert!(heavy::analyze(sql).is_ok());
}

#[rstest::rstest]
#[case("select json_object('x': 1)")]
#[case("select merge_action()")]
#[case("select x from xmltable('/x' passing '<x/>' columns x text path '.') as t")]
#[case("select x from json_table('{}', '$' columns(x text path '$.x')) as j")]
#[case(
    "with c as (merge into missing using other on true when matched then do nothing returning merge_action()) select 1"
)]
fn unsupported_raw_syntax_retains_analysis_error(#[case] sql: &str) {
    assert!(heavy::analyze(sql).is_err(), "{sql}");
}
