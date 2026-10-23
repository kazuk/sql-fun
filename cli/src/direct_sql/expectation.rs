use std::collections::BTreeSet;

use ::anyhow::Context;
use ::serde_json::Value;
use ::sql_fun_client::payloads::{AnalyzeQueryResponse, ResultSetColumn};

use super::input::{SqlInputs, load_value};

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Expectation {
    diagnostics: BTreeSet<String>,
    #[serde(default)]
    columns: Vec<ExpectedColumn>,
}

#[derive(Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpectedColumn {
    name: String,
    #[serde(rename = "type")]
    column_type: Option<ExpectedType>,
    nullability: ExpectedNullability,
}
#[derive(Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpectedType {
    schema_name: String,
    local_name: String,
    is_array: bool,
}
#[derive(Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum ExpectedNullability {
    NotNull,
    Nullable,
    Unknown,
}

impl ExpectedColumn {
    fn from_api(column: &ResultSetColumn) -> Result<Self, ::anyhow::Error> {
        // Project only fields the API actually supplies; absence is not a wildcard.
        Ok(::serde_json::from_value(::serde_json::json!({
            "name": column.name(), "type": column.column_type(), "nullability": column.nullability().contract_name(),
        }))?)
    }
}
impl Expectation {
    pub(super) fn load(reference: &str, inputs: &SqlInputs) -> Result<Vec<Self>, ::anyhow::Error> {
        let value = load_value(reference)?;
        Self::for_inputs(value, inputs.is_array(), inputs.queries().len())
    }
    fn for_inputs(value: Value, array: bool, count: usize) -> Result<Vec<Self>, ::anyhow::Error> {
        if array {
            let values = value
                .as_array()
                .context("a SQL array requires an array of per-input expectations")?;
            ::anyhow::ensure!(
                values.len() == count,
                "expectation count {} does not match SQL input count {count}",
                values.len()
            );
            values
                .iter()
                .map(|value| Self::parse(value.clone()))
                .collect()
        } else {
            Ok(vec![Self::parse(value)?])
        }
    }
    fn parse(value: Value) -> Result<Self, ::anyhow::Error> {
        if value.is_array() {
            Ok(Self {
                diagnostics: ::serde_json::from_value(value)?,
                columns: Vec::new(),
            })
        } else {
            Ok(::serde_json::from_value(value)?)
        }
    }
    pub(super) fn differences(
        &self,
        response: &AnalyzeQueryResponse,
    ) -> Result<Vec<String>, ::anyhow::Error> {
        let items = response.queries().items();
        if items.len() != 1 {
            return Ok(vec![format!(
                "expected exactly one statement result, actual {}",
                items.len()
            )]);
        }
        let item = &items[0];
        let actual: BTreeSet<_> = item
            .probrems()
            .items()
            .iter()
            .map(|diagnostic| diagnostic.code().to_owned())
            .collect();
        let mut differences = Vec::new();
        let missing: Vec<_> = self.diagnostics.difference(&actual).collect();
        let unexpected: Vec<_> = actual.difference(&self.diagnostics).collect();
        if !missing.is_empty() || !unexpected.is_empty() {
            differences.push(format!(
                "diagnostics: missing {missing:?}; unexpected {unexpected:?}"
            ));
        }
        let mut columns: Vec<_> = item.result_columns().items().iter().collect();
        columns.sort_by_key(|column| *column.ordinal());
        if columns.len() != self.columns.len() {
            differences.push(format!(
                "result column count: expected {}, actual {}",
                self.columns.len(),
                columns.len()
            ));
        }
        for (index, (expected, actual)) in self.columns.iter().zip(columns).enumerate() {
            let actual = ExpectedColumn::from_api(actual)?;
            if *expected != actual {
                differences.push(format!(
                    "column {index}: expected {}; actual {}",
                    ::serde_json::to_string(expected)?,
                    ::serde_json::to_string(&actual)?
                ));
            }
        }
        Ok(differences)
    }
}

#[cfg(test)]
mod tests {
    use super::Expectation;
    #[rstest::rstest]
    #[case::empty_codes("[]", true)]
    #[case::object(r#"{"diagnostics":[]}"#, true)]
    #[case::missing_codes("{}", false)]
    #[case::unknown(r#"{"diagnostics":[],"column":[]}"#, false)]
    #[case::bad_codes(r#"{"diagnostics":[1]}"#, false)]
    #[case::missing_nullability(r#"{"diagnostics":[],"columns":[{"name":"x"}]}"#, false)]
    #[case::absent_type(
        r#"{"diagnostics":[],"columns":[{"name":"x","nullability":"unknown"}]}"#,
        true
    )]
    #[case::missing_array_choice(r#"{"diagnostics":[],"columns":[{"name":"x","type":{"schema_name":"pg_catalog","local_name":"int4"},"nullability":"not_null"}]}"#, false)]
    fn expectation_contract(#[case] value: &str, #[case] valid: bool) {
        assert_eq!(
            Expectation::parse(::serde_json::from_str(value).unwrap()).is_ok(),
            valid
        );
    }
    #[test]
    fn array_cardinality_is_validated_before_execution() {
        assert!(Expectation::for_inputs(::serde_json::json!([[]]), true, 2).is_err());
        assert!(Expectation::for_inputs(::serde_json::json!({"diagnostics":[]}), true, 1).is_err());
        assert!(Expectation::for_inputs(::serde_json::json!([[]]), false, 1).is_err());
    }
}

#[cfg(test)]
mod test_helpers;
#[cfg(test)]
mod test_comparison {
    use super::{Expectation, test_helpers};
    #[rstest::rstest]
    #[case::reordered_and_duplicate(r#"["a","b","a"]"#, vec!["b", "a", "b"], true)]
    #[case::missing(r#"["a","b"]"#, vec!["a"], false)]
    #[case::unexpected(r#"["a"]"#, vec!["a", "b"], false)]
    #[case::empty("[]", vec![], true)]
    #[case::warning_is_not_empty("[]", vec!["warning_code"], false)]
    fn diagnostic_comparison_is_set_equality(
        #[case] expected: &str,
        #[case] actual: Vec<&str>,
        #[case] passes: bool,
    ) -> ::testresult::TestResult {
        let expected = Expectation::parse(::serde_json::from_str(expected)?)?;
        assert_eq!(
            expected
                .differences(&test_helpers::response(&actual, Vec::new()))?
                .is_empty(),
            passes
        );
        Ok(())
    }
    #[rstest::rstest]
    #[case::omitted("{}")]
    #[case::explicit_empty(r#"{"columns":[]}"#)]
    fn columns_are_always_asserted(#[case] fields: &str) -> ::testresult::TestResult {
        let mut expected: ::serde_json::Value = ::serde_json::from_str(fields)?;
        expected["diagnostics"] = ::serde_json::json!([]);
        let expected = Expectation::parse(expected)?;
        let response = test_helpers::response(
            &[],
            vec![test_helpers::column("id", true, false, Some(true))],
        );
        assert_ne!(expected.differences(&response)?, Vec::<String>::new());
        Ok(())
    }
    #[rstest::rstest]
    #[case::equal("id", true, false, Some(true), true)]
    #[case::name("changed", true, false, Some(true), false)]
    #[case::absent_type("id", false, false, Some(true), false)]
    #[case::array("id", true, true, Some(true), false)]
    #[case::nullable("id", true, false, Some(false), false)]
    #[case::unknown("id", true, false, None, false)]
    fn column_comparison_checks_every_contract_field(
        #[case] name: &str,
        #[case] typed: bool,
        #[case] array: bool,
        #[case] nullability: Option<bool>,
        #[case] passes: bool,
    ) -> ::testresult::TestResult {
        let expected = Expectation::parse(
            ::serde_json::json!({"diagnostics":[],"columns":[{"name":"id","type":{"schema_name":"pg_catalog","local_name":"int4","is_array":false},"nullability":"not_null"}]}),
        )?;
        let response = test_helpers::response(
            &[],
            vec![test_helpers::column(name, typed, array, nullability)],
        );
        assert_eq!(expected.differences(&response)?.is_empty(), passes);
        Ok(())
    }
    #[rstest::rstest]
    #[case::nullable("nullable", Some(false))]
    #[case::unknown("unknown", None)]
    fn absent_type_and_all_nullability_states_can_be_asserted(
        #[case] nullability: &str,
        #[case] actual: Option<bool>,
    ) -> ::testresult::TestResult {
        let expected = Expectation::parse(
            ::serde_json::json!({"diagnostics":[],"columns":[{"name":"id","nullability":nullability}]}),
        )?;
        assert_eq!(
            expected.differences(&test_helpers::response(
                &[],
                vec![test_helpers::column("id", false, false, actual)]
            ))?,
            Vec::<String>::new()
        );
        Ok(())
    }
}
