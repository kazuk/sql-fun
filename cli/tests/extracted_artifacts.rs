//! Producer-output contracts for officially extracted `PostgreSQL` artifacts.
mod support;

#[rstest::rstest]
#[case::seventeen("17")]
#[case::eighteen("18")]
fn master_payloads_preserve_identities_relationships_and_defaults(#[case] version: &str) {
    let masters = support::MasterDefinitions::builtin(version);
    masters.assert_contract();
}
