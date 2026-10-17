//! Real-process regression coverage for server idle shutdown and configuration.

mod test_helpers {
    pub(super) mod heavy;
}

use self::test_helpers::heavy;

use std::time::Duration;

#[test]
fn server_without_clients_exits_cleans_up_and_can_restart() {
    let mut server = heavy::Server::start("1", "10");
    server.assert_idle_exit();
    server.restart();
    let client = server.authenticate();
    drop(client);
    server.assert_idle_exit();
}

#[test]
fn connected_client_prevents_shutdown_until_final_disconnect() {
    let mut server = heavy::Server::start("1", "10");
    let first = server.authenticate();
    let second = server.authenticate();
    server.assert_running_for(Duration::from_millis(1_200));
    drop(first);
    server.assert_running_for(Duration::from_millis(1_200));
    drop(second);
    // The old startup deadline has passed; disconnection must start a new one.
    server.assert_running_for(Duration::from_millis(300));
    server.assert_idle_exit();
}

#[test]
fn reconnect_cancels_pending_shutdown() {
    let mut server = heavy::Server::start("2", "10");
    drop(server.authenticate());
    server.assert_running_for(Duration::from_millis(300));
    let client = server.authenticate();
    server.assert_running_for(Duration::from_millis(2_200));
    drop(client);
    server.assert_running_for(Duration::from_millis(300));
    server.assert_idle_exit();
}

#[test]
fn zero_disables_shutdown_before_and_after_connections() {
    let mut server = heavy::Server::start("0", "10");
    server.assert_running_for(Duration::from_millis(1_200));
    drop(server.authenticate());
    server.assert_running_for(Duration::from_millis(1_200));
}

#[test]
fn read_timeout_remains_independent_and_then_starts_server_idle_period() {
    let mut server = heavy::Server::start("1", "2");
    let mut client = server.authenticate();
    server.assert_running_for(Duration::from_millis(1_200));
    client.assert_closed();
    server.assert_running_for(Duration::from_millis(300));
    server.assert_idle_exit();
}

#[test]
fn socket_counts_as_connected_before_authentication() {
    let mut server = heavy::Server::start("1", "10");
    let client = server.connect();
    server.assert_running_for(Duration::from_millis(1_200));
    drop(client);
    server.assert_idle_exit();
}

#[test]
fn failed_authentication_releases_connection() {
    let mut server = heavy::Server::start("1", "10");
    let mut client = server.connect();
    client.reject_authentication();
    server.assert_running_for(Duration::from_millis(300));
    server.assert_idle_exit();
}

#[test]
fn environment_configures_timeout() {
    let mut server = heavy::Server::with_options(&[], Some("1"), "10");
    server.assert_idle_exit();
}

#[test]
fn argument_overrides_environment() {
    let mut server = heavy::Server::with_options(&["--server-idle-timeout", "1"], Some("0"), "10");
    server.assert_idle_exit();
}

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
