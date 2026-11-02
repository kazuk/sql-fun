//! Golden inventory contract shared by source and installed-package execution.

pub(super) fn verify() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::var("SQL_FUN_E2E_INVENTORY")?;
    let mut actual: ::toml::Value = ::toml::from_str(&std::fs::read_to_string(path)?)?;
    // The runner explicitly selects a fresh schema variant per invocation.
    actual["schema_variant"] = ::toml::Value::String("fixture".to_owned());
    let expected: ::toml::Value = ::toml::from_str(include_str!("../inventory.toml"))?;
    assert_eq!(
        actual, expected,
        "generated inventory differs from the maintained fixture"
    );
    Ok(())
}
