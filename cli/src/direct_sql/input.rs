use std::path::Path;

use ::anyhow::Context;
use ::serde_json::Value;

pub(super) struct SqlInputs {
    source: String,
    queries: Vec<String>,
    array: bool,
    names: Vec<String>,
}
impl SqlInputs {
    pub(super) fn inline(sql: &str) -> Self {
        Self {
            source: "<inline SQL>".to_owned(),
            queries: vec![sql.to_owned()],
            array: false,
            names: Vec::new(),
        }
    }
    pub(super) fn load(reference: &str) -> Result<Self, ::anyhow::Error> {
        let (queries, array) = if reference.contains('#') {
            Self::selected_queries(&load_value(reference)?)?
        } else {
            (
                vec![
                    std::fs::read_to_string(reference)
                        .with_context(|| format!("cannot read SQL input {reference}"))?,
                ],
                false,
            )
        };
        Ok(Self {
            source: reference.to_owned(),
            queries,
            array,
            names: Vec::new(),
        })
    }
    pub(super) fn named(reference: &str) -> Self {
        Self {
            source: reference.to_owned(),
            queries: Vec::new(),
            array: true,
            names: Vec::new(),
        }
    }
    pub(super) fn push_named(&mut self, name: &str, sql: &str) {
        self.names.push(name.to_owned());
        self.queries.push(sql.to_owned());
    }
    fn selected_queries(value: &Value) -> Result<(Vec<String>, bool), ::anyhow::Error> {
        if let Some(sql) = value.as_str() {
            return Ok((vec![sql.to_owned()], false));
        }
        let values = value
            .as_array()
            .context("SQL selection must be a string or a string array")?;
        ::anyhow::ensure!(!values.is_empty(), "SQL input array must not be empty");
        let queries = values
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .map(str::to_owned)
                    .context("SQL array elements must be strings")
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok((queries, true))
    }
    pub(super) const fn is_array(&self) -> bool {
        self.array
    }
    pub(super) fn queries(&self) -> &[String] {
        &self.queries
    }
    pub(super) fn identity(&self, index: usize) -> String {
        if let Some(name) = self.names.get(index) {
            format!("{}::{name}", self.source)
        } else if self.array {
            format!("{}[{index}]", self.source)
        } else {
            self.source.clone()
        }
    }
    pub(super) fn diagnostic_identity(&self, index: usize) -> String {
        if let Some(name) = self.names.get(index) {
            format!("{}[{index}]::{name}", self.source)
        } else {
            self.identity(index)
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::direct_sql::input::SqlInputs;

    #[rstest::rstest]
    #[case::scalar(r#""SELECT 1""#, true)]
    #[case::array(r#"["SELECT 1", "SELECT 2"]"#, true)]
    #[case::empty("[]", false)]
    #[case::mixed(r#"["SELECT 1", 2]"#, false)]
    #[case::object("{}", false)]
    fn selected_input_contract(#[case] value: &str, #[case] valid: bool) {
        assert_eq!(
            SqlInputs::selected_queries(&::serde_json::from_str(value).unwrap()).is_ok(),
            valid
        );
    }
}

#[cfg(test)]
mod test_selection {
    use crate::{direct_sql::input::SqlInputs, test_helpers::direct_sql::StructuredFile};
    #[rstest::rstest]
    fn formats_preserve_scalar_and_array_identity(
        #[values("json", "toml", "yaml")] format: &str,
    ) -> ::testresult::TestResult {
        let file = StructuredFile::sql(format)?;
        let array = SqlInputs::load(&file.selection("$.queries"))?;
        assert!(array.is_array());
        assert_eq!(array.queries(), ["SELECT 1", "SELECT 2"]);
        let scalar = SqlInputs::load(&file.selection("$.scalar"))?;
        assert!(!scalar.is_array());
        assert_eq!(scalar.queries(), ["SELECT 3"]);
        Ok(())
    }
    #[rstest::rstest]
    #[case::absent("$.absent")]
    #[case::multiple("$.queries[*]")]
    #[case::invalid("$[")]
    fn invalid_selectors_are_not_flattened(#[case] selector: &str) -> ::testresult::TestResult {
        let file = StructuredFile::sql("json")?;
        assert!(SqlInputs::load(&file.selection(selector)).is_err());
        Ok(())
    }
}

/// SQL and expectation selectors have the same exactly-one-node contract.
pub(super) fn load_value(reference: &str) -> Result<Value, ::anyhow::Error> {
    let (filename, selector) = reference
        .split_once('#')
        .map_or((reference, None), |(file, selector)| (file, Some(selector)));
    let path = Path::new(filename);
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("cannot read structured input {filename}"))?;
    let extension = if let Some(extension) = path.extension() {
        extension.to_str()
    } else {
        None
    };
    let value: Value = match extension {
        Some("json") => ::serde_json::from_str(&text)?,
        Some("toml") => ::toml::from_str(&text)?,
        Some("yaml") => ::serde_saphyr::from_str(&text)?,
        _ => ::anyhow::bail!(
            "structured input {filename} requires a .json, .toml, or .yaml extension"
        ),
    };
    if let Some(selector) = selector {
        Ok(::serde_json_path::JsonPath::parse(selector)?
            .query(&value)
            .exactly_one()
            .with_context(|| format!("selector {reference} must match exactly one node"))?
            .clone())
    } else {
        Ok(value)
    }
}
