mod test_helpers;
use self::test_helpers::{CatalogFixture, member};

use super::BtreeOperatorMembership;
use crate::sem::{FullName, TypeReference};

#[test]
fn cross_type_custom_operator_retains_multiple_interpretations() {
    let fixture = CatalogFixture::memberships(&[member("Family A", 1), member("Family B", 5)]);
    let payload = fixture.load().unwrap().unwrap();
    let operator = FullName::with_schema("Custom 雪", "<~>");
    let left = TypeReference::from_master_type_name("int4");
    let right = TypeReference::from_master_type_name("int8");
    let matches = payload.lookup(&operator, &left, &right);
    assert_eq!(matches.len(), 2);
    assert_eq!(
        payload
            .negator_lookup(&FullName::with_schema("Custom 雪", "!~>"), &left, &right)
            .len(),
        2
    );
    assert_eq!(
        payload.negator_lookup(
            &FullName::with_schema("Custom 雪", "different reverse edge"),
            &left,
            &right
        ),
        [] as [&BtreeOperatorMembership; 0]
    );
    assert_eq!(matches[0].strategy(), 1);
    assert_eq!(matches[1].strategy(), 5);
    assert!(matches[0].is_comparison());
    assert_eq!(matches[0].family().schema(), "Elsewhere");
    assert_eq!(
        payload.lookup(&operator, &right, &left),
        [] as [&BtreeOperatorMembership; 0]
    );
    assert_eq!(
        payload.lookup(&FullName::from_local_name("<~>"), &left, &right),
        [] as [&BtreeOperatorMembership; 0]
    );
}

#[rstest::rstest]
#[case::unsupported_version(r#"{"format_version":2,"memberships":[]}"#)]
#[case::malformed("{")]
fn present_invalid_metadata_is_not_a_negative_lookup(#[case] content: &str) {
    let fixture = CatalogFixture::with_payload(content);
    assert!(fixture.load().is_err());
}

#[test]
fn legacy_absence_differs_from_completed_empty_catalog() {
    assert!(CatalogFixture::legacy().load().unwrap().is_none());
    let fixture = CatalogFixture::with_payload(r#"{"format_version":1,"memberships":[]}"#);
    let payload = fixture.load().unwrap().unwrap();
    let input = TypeReference::from_master_type_name("int4");
    assert_eq!(
        payload.lookup(&FullName::with_schema("pg_catalog", "~~"), &input, &input),
        [] as [&BtreeOperatorMembership; 0]
    );
}

#[test]
fn purpose_is_retained_without_fabricating_comparison() {
    let mut value = member("Family A", 1);
    value["purpose"] = serde_json::json!("o");
    let membership: super::BtreeOperatorMembership = serde_json::from_value(value).unwrap();
    assert_eq!(membership.purpose(), "o");
    assert!(!membership.is_comparison());
}

#[test]
fn missing_negator_metadata_is_not_declared_absence() {
    let mut value = member("Family A", 1);
    value.as_object_mut().unwrap().remove("negator");
    assert!(serde_json::from_value::<BtreeOperatorMembership>(value).is_err());
}

#[rstest::rstest]
#[case::unknown_strategy("strategy", serde_json::json!(9))]
#[case::unknown_purpose("purpose", serde_json::json!("unknown"))]
#[case::different_access_method("family", serde_json::json!({"schema":"Elsewhere","name":"Family A","access_method":"gin"}))]
fn unsupported_member_metadata_is_rejected(
    #[case] field: &str,
    #[case] invalid: serde_json::Value,
) {
    let mut value = member("Family A", 1);
    value[field] = invalid;
    assert!(CatalogFixture::memberships(&[value]).load().is_err());
}
