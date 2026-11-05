use std::collections::BTreeSet;

use ::anyhow::Context;
use ::serde_json::Value;
use ::sql_fun_client::payloads::{AnalyzeQueryResponse, ResultSetColumn};

use crate::direct_sql::input::{SqlInputs, load_value};

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Expectation {
    diagnostics: BTreeSet<String>,
    #[serde(default)]
    columns: Vec<ExpectedColumn>,
}

#[derive(Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpectedColumn {
    name: String,
    #[serde(rename = "type")]
    column_type: Option<ExpectedType>,
    nullability: ExpectedNullability,
}
#[derive(Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpectedType {
    schema_name: String,
    local_name: String,
    is_array: bool,
}
#[derive(Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum ExpectedNullability {
    NotNull,
    Nullable,
    Unknown,
}

impl std::fmt::Display for ExpectedColumn {
    fn fmt(&self, output: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(output, "    name: {:?}", self.name)?;
        if let Some(column_type) = &self.column_type {
            writeln!(output, "    type: {column_type}")?;
        } else {
            writeln!(output, "    type: absent")?;
        }
        write!(output, "    nullability: {}", self.nullability)
    }
}
impl std::fmt::Display for ExpectedType {
    fn fmt(&self, output: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            output,
            "{:?}.{:?}{}",
            self.schema_name,
            self.local_name,
            if self.is_array { "[]" } else { "" }
        )
    }
}
impl std::fmt::Display for ExpectedNullability {
    fn fmt(&self, output: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        output.write_str(match self {
            Self::NotNull => "not_null",
            Self::Nullable => "nullable",
            Self::Unknown => "unknown",
        })
    }
}

fn code_list(codes: &[String]) -> String {
    if codes.is_empty() {
        "    (none)".to_owned()
    } else {
        codes
            .iter()
            .map(|code| format!("    {code:?}"))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Structured assertion mismatches; formatting never determines equality.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Difference {
    StatementCount(usize),
    Diagnostics {
        missing: Vec<String>,
        unexpected: Vec<String>,
    },
    ColumnCount {
        expected: usize,
        actual: usize,
    },
    ColumnField {
        index: usize,
        field: &'static str,
        expected: String,
        actual: String,
    },
    MissingColumn {
        index: usize,
        column: String,
    },
    UnexpectedColumn {
        index: usize,
        column: String,
    },
}

impl Difference {
    pub(super) fn includes_diagnostic(&self, code: &str) -> bool {
        match self {
            Self::StatementCount(_) => true,
            Self::Diagnostics { unexpected, .. } => unexpected.iter().any(|item| item == code),
            _ => false,
        }
    }

    pub(super) fn format(&self, color: bool) -> String {
        use ::owo_colors::OwoColorize;
        let expected_value = |value: &str| {
            if color {
                value.green().to_string()
            } else {
                value.to_owned()
            }
        };
        let actual_value = |value: &str| {
            if color {
                value.red().to_string()
            } else {
                value.to_owned()
            }
        };
        match self {
            Self::StatementCount(actual) => {
                format!("expected exactly one statement result, actual {actual}")
            }
            Self::Diagnostics {
                missing,
                unexpected,
            } => format!(
                "Diagnostics:\n  Missing:\n{}\n  Unexpected:\n{}",
                expected_value(&code_list(missing)),
                actual_value(&code_list(unexpected))
            ),
            Self::ColumnCount { expected, actual } => {
                format!("Result column count: expected {expected}, actual {actual}")
            }
            Self::ColumnField {
                index,
                field,
                expected,
                actual,
            } => format!(
                "Column {index} · {field}\n  Expected: {}\n  Actual:   {}",
                expected_value(expected),
                actual_value(actual)
            ),
            Self::MissingColumn { index, column } => format!(
                "Column {index} · missing\n  Expected:\n{}",
                expected_value(column)
            ),
            Self::UnexpectedColumn { index, column } => format!(
                "Column {index} · unexpected\n  Actual:\n{}",
                actual_value(column)
            ),
        }
    }
}

impl ExpectedColumn {
    fn add_differences(&self, actual: &Self, index: usize, differences: &mut Vec<Difference>) {
        if self.name != actual.name {
            differences.push(Difference::ColumnField {
                index,
                field: "name",
                expected: format!("{:?}", self.name),
                actual: format!("{:?}", actual.name),
            });
        }
        if self.column_type != actual.column_type {
            let display_type = |value: &Option<ExpectedType>| {
                value
                    .as_ref()
                    .map_or_else(|| "absent".to_owned(), ToString::to_string)
            };
            differences.push(Difference::ColumnField {
                index,
                field: "type",
                expected: display_type(&self.column_type),
                actual: display_type(&actual.column_type),
            });
        }
        if self.nullability != actual.nullability {
            differences.push(Difference::ColumnField {
                index,
                field: "nullability",
                expected: self.nullability.to_string(),
                actual: actual.nullability.to_string(),
            });
        }
    }

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
    pub(super) fn parse(value: Value) -> Result<Self, ::anyhow::Error> {
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
    ) -> Result<Vec<Difference>, ::anyhow::Error> {
        let items = response.queries().items();
        if items.len() != 1 {
            return Ok(vec![Difference::StatementCount(items.len())]);
        }
        let item = &items[0];
        let actual: BTreeSet<_> = item
            .probrems()
            .items()
            .iter()
            .map(|diagnostic| diagnostic.code().to_owned())
            .collect();
        let mut differences = Vec::new();
        let missing: Vec<_> = self.diagnostics.difference(&actual).cloned().collect();
        let unexpected: Vec<_> = actual.difference(&self.diagnostics).cloned().collect();
        if !missing.is_empty() || !unexpected.is_empty() {
            differences.push(Difference::Diagnostics {
                missing,
                unexpected,
            });
        }
        let mut columns: Vec<_> = item.result_columns().items().iter().collect();
        columns.sort_by_key(|column| *column.ordinal());
        let actual_columns = columns
            .into_iter()
            .map(ExpectedColumn::from_api)
            .collect::<Result<Vec<_>, _>>()?;
        if actual_columns.len() != self.columns.len() {
            differences.push(Difference::ColumnCount {
                expected: self.columns.len(),
                actual: actual_columns.len(),
            });
        }
        for (index, (expected, actual)) in self.columns.iter().zip(&actual_columns).enumerate() {
            expected.add_differences(actual, index, &mut differences);
        }
        for (index, column) in self.columns.iter().enumerate().skip(actual_columns.len()) {
            differences.push(Difference::MissingColumn {
                index,
                column: column.to_string(),
            });
        }
        for (index, column) in actual_columns.iter().enumerate().skip(self.columns.len()) {
            differences.push(Difference::UnexpectedColumn {
                index,
                column: column.to_string(),
            });
        }
        Ok(differences)
    }
}

#[cfg(test)]
mod tests {
    use crate::direct_sql::expectation::Expectation;
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
    use crate::direct_sql::expectation::{Expectation, test_helpers};

    #[test]
    fn differences_separate_codes_and_expected_actual_column_fields() -> ::testresult::TestResult {
        let expected = Expectation::parse(::serde_json::json!({
            "diagnostics":["missing_b","missing_a"],
            "columns":[{"name":"expected_id", "type":{"schema_name":"app", "local_name":"identifier", "is_array":true}, "nullability":"nullable"}]
        }))?;
        let actual = test_helpers::response(
            &["unexpected_b", "unexpected_a"],
            vec![test_helpers::column("actual_id", true, false, Some(true))],
        );
        assert_eq!(
            expected
                .differences(&actual)?
                .iter()
                .map(|difference| difference.format(false))
                .collect::<Vec<_>>(),
            vec![
                concat!(
                    "Diagnostics:\n  Missing:\n    \"missing_a\"\n    \"missing_b\"",
                    "\n  Unexpected:\n    \"unexpected_a\"\n    \"unexpected_b\""
                ),
                "Column 0 · name\n  Expected: \"expected_id\"\n  Actual:   \"actual_id\"",
                "Column 0 · type\n  Expected: \"app\".\"identifier\"[]\n  Actual:   \"pg_catalog\".\"int4\"",
                "Column 0 · nullability\n  Expected: nullable\n  Actual:   not_null",
            ]
        );
        Ok(())
    }

    #[test]
    fn differences_show_absent_type_unknown_nullability_and_empty_code_side()
    -> ::testresult::TestResult {
        let expected = Expectation::parse(::serde_json::json!({
            "diagnostics":[],"columns":[{"name":"id","nullability":"unknown"}]
        }))?;
        let actual = test_helpers::response(
            &["warning_code"],
            vec![test_helpers::column("id", true, false, Some(true))],
        );
        let differences = expected.differences(&actual)?;
        assert!(
            differences[0]
                .format(false)
                .contains("Missing:\n    (none)\n  Unexpected:\n    \"warning_code\"")
        );
        assert!(differences[1].format(false).contains("Expected: absent"));
        assert!(differences[2].format(false).contains("Expected: unknown"));
        Ok(())
    }

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
        assert_ne!(
            expected.differences(&response)?,
            Vec::<super::Difference>::new()
        );
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
            Vec::<super::Difference>::new()
        );
        Ok(())
    }
}
