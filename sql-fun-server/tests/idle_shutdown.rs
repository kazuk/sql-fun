//! Real-process argument and help checks for server idle configuration.

mod test_helpers {
    pub(super) mod heavy;
}

use self::test_helpers::heavy;

#[rstest::rstest]
#[case::negative("-1")]
#[case::malformed("never")]
#[case::overflow("18446744073709551615")]
#[case::outside_supported_range("4294967296")]
fn invalid_timeout_is_rejected(#[case] value: &str, #[values(false, true)] environment: bool) {
    let argument = format!("--server-idle-timeout={value}");
    let output = if environment {
        heavy::Server::configuration_output(&[], Some(value))
    } else {
        heavy::Server::configuration_output(&[&argument], None)
    };
    assert!(!output.status.success());
    let diagnostic = String::from_utf8(output.stderr).unwrap();
    assert!(diagnostic.contains("--server-idle-timeout"), "{diagnostic}");
}

#[test]
fn help_documents_default_and_disable_setting() {
    let output = heavy::Server::configuration_output(&["--help"], None);
    assert!(output.status.success());
    let help = String::from_utf8(output.stdout).unwrap();
    assert!(help.contains("--server-idle-timeout"), "{help}");
    assert!(help.contains("SQL_FUN_SERVER_IDLE_TIMEOUT"), "{help}");
    assert!(help.contains("[default: 180]"), "{help}");
    assert!(help.contains("0 disables it"), "{help}");
}
