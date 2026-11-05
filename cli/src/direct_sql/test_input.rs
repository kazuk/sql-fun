use crate::direct_sql::{
    expectation::Expectation,
    input::{SqlInputs, load_value},
};
use std::collections::BTreeSet;

use ::anyhow::Context;
use ::serde_json::Value;

pub(super) struct TestInputs {
    sql: SqlInputs,
    expectations: Vec<Expectation>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct NamedCase {
    name: String,
    sql: String,
    expected: Value,
}

impl TestInputs {
    pub(super) fn load(reference: &str, expect: Option<&str>) -> Result<Self, ::anyhow::Error> {
        if let Some(expect) = expect {
            let sql = SqlInputs::load(reference)?;
            let expectations = Expectation::load(expect, &sql)?;
            Ok(Self { sql, expectations })
        } else {
            Self::from_cases(reference, &load_value(reference)?)
        }
    }
    fn from_cases(reference: &str, value: &Value) -> Result<Self, ::anyhow::Error> {
        let values = value
            .as_array()
            .context("test input without --expect must be a named case array")?;
        ::anyhow::ensure!(!values.is_empty(), "test case array must not be empty");
        let mut result = Self {
            sql: SqlInputs::named(reference),
            expectations: Vec::new(),
        };
        let mut names = BTreeSet::new();
        for (index, value) in values.iter().enumerate() {
            ::anyhow::ensure!(
                value.is_object(),
                "test case {reference}[{index}] must be an object"
            );
            let case: NamedCase = ::serde_json::from_value(value.clone())
                .with_context(|| format!("invalid test case {reference}[{index}]"))?;
            ::anyhow::ensure!(
                !case.name.is_empty(),
                "test case {reference}[{index}] name must not be empty"
            );
            ::anyhow::ensure!(
                names.insert(case.name.clone()),
                "duplicate test case name {:?} at {reference}[{index}]",
                case.name
            );
            let expectation = Expectation::parse(case.expected).with_context(|| {
                format!(
                    "invalid expected contract for {reference}[{index}]::{name}",
                    name = case.name
                )
            })?;
            result.sql.push_named(&case.name, &case.sql);
            result.expectations.push(expectation);
        }
        Ok(result)
    }
    pub(super) const fn sql(&self) -> &SqlInputs {
        &self.sql
    }
    pub(super) fn expectations(&self) -> &[Expectation] {
        &self.expectations
    }
}

#[cfg(test)]
mod tests {
    use crate::direct_sql::test_input::TestInputs;

    #[rstest::rstest]
    #[case::empty("[]")]
    #[case::object("{}")]
    #[case::string(r#""query""#)]
    #[case::missing_name(r#"[{"sql":"SELECT 1","expected":[]}]"#)]
    #[case::empty_name(r#"[{"name":"","sql":"SELECT 1","expected":[]}]"#)]
    #[case::wrong_name(r#"[{"name":1,"sql":"SELECT 1","expected":[]}]"#)]
    #[case::missing_sql(r#"[{"name":"a","expected":[]}]"#)]
    #[case::wrong_sql(r#"[{"name":"a","sql":1,"expected":[]}]"#)]
    #[case::missing_expected(r#"[{"name":"a","sql":"SELECT 1"}]"#)]
    #[case::bad_expected(r#"[{"name":"a","sql":"SELECT 1","expected":{}}]"#)]
    #[case::null_expected(r#"[{"name":"a","sql":"SELECT 1","expected":null}]"#)]
    #[case::unknown(r#"[{"name":"a","sql":"SELECT 1","expected":[],"extra":1}]"#)]
    #[case::null_case("[null]")]
    #[case::positional_case(r#"[["name","SELECT 1",[]]]"#)]
    #[case::mixed(r#"[{"name":"a","sql":"SELECT 1","expected":[]},"SELECT 2"]"#)]
    #[case::duplicate(r#"[{"name":"a","sql":"SELECT 1","expected":[]},{"name":"a","sql":"SELECT 2","expected":[]}]"#)]
    fn rejects_invalid_case_arrays(#[case] value: &str) -> ::testresult::TestResult {
        assert!(TestInputs::from_cases("cases.json", &::serde_json::from_str(value)?).is_err());
        Ok(())
    }

    #[test]
    fn preserves_named_order_source_and_expectation_semantics() -> ::testresult::TestResult {
        let inputs = TestInputs::from_cases(
            "cases.json#$.tests",
            &::serde_json::json!([
                {"name":"same", "sql":"", "expected":[]},
                {"name":"Same", "sql":"SELECT 1", "expected":{"diagnostics":[]}}
            ]),
        )?;
        assert_eq!(inputs.sql().queries(), ["", "SELECT 1"]);
        assert_eq!(inputs.sql().identity(1), "cases.json#$.tests::Same");
        assert_eq!(inputs.expectations().len(), 2);
        Ok(())
    }
}

#[cfg(test)]
mod test_selection {
    use crate::{direct_sql::test_input::TestInputs, test_helpers::direct_sql::StructuredFile};

    #[rstest::rstest]
    fn selected_cases_use_existing_structured_formats(
        #[values("json", "toml", "yaml")] format: &str,
    ) -> ::testresult::TestResult {
        let file = StructuredFile::named_cases(format)?;
        let reference = file.selection("$.cases");
        let inputs = TestInputs::load(&reference, None)?;
        assert_eq!(inputs.sql().identity(0), format!("{reference}::named"));
        assert_eq!(inputs.sql().queries(), ["SELECT 1"]);
        Ok(())
    }

    #[rstest::rstest]
    #[case::missing("$.absent")]
    #[case::multiple("$..*")]
    #[case::invalid("$[")]
    #[case::case_not_array("$.cases[0]")]
    fn rejects_invalid_case_selections(#[case] selector: &str) -> ::testresult::TestResult {
        let file = StructuredFile::named_cases("json")?;
        assert!(TestInputs::load(&file.selection(selector), None).is_err());
        Ok(())
    }
}
