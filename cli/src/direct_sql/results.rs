use std::io::Write;

use ::sql_fun_client::payloads::{AnalyzeQueryResponse, ObjectKind, QueryKind, TypeReference};

/// Display completed collections, preserving input and statement identity.
pub(super) fn write_response(
    output: &mut impl Write,
    source: &str,
    response: &AnalyzeQueryResponse,
) -> Result<(), std::io::Error> {
    for (index, statement) in response.queries().items().iter().enumerate() {
        writeln!(output, "{source} (statement {index})")?;
        let kind = statement
            .query_kind()
            .as_ref()
            .map_or("unavailable", AnalysisLabel::label);
        writeln!(output, "  Query kind: {kind}")?;
        writeln!(output, "  Complete: {}", statement.complete())?;
        writeln!(output, "  Executable SQL:")?;
        if let Some(sql) = statement.execute_sql() {
            for line in sql.lines() {
                writeln!(output, "    {line}")?;
            }
        } else {
            writeln!(output, "    unavailable")?;
        }
        let mut parameters: Vec<_> = statement.parameters().items().iter().collect();
        parameters.sort_by_key(|parameter| *parameter.parameter_index());
        writeln!(output, "  Parameters ({}):", parameters.len())?;
        for parameter in parameters {
            writeln!(
                output,
                "    ${} {}: {}; {}",
                parameter.parameter_index(),
                parameter.name(),
                type_name(parameter.parameter_type().as_ref()),
                parameter.nullability().contract_name(),
            )?;
        }
        let mut columns: Vec<_> = statement.result_columns().items().iter().collect();
        columns.sort_by_key(|column| *column.ordinal());
        writeln!(output, "  Result columns ({}):", columns.len())?;
        for column in columns {
            writeln!(
                output,
                "    [{}] {}: {}; {}",
                column.ordinal(),
                column.name(),
                type_name(column.column_type().as_ref()),
                column.nullability().contract_name(),
            )?;
        }
        writeln!(
            output,
            "  Column access ({}):",
            statement.column_access().items().len()
        )?;
        for access in statement.column_access().items() {
            let relation = access.relation();
            let relation_name = relation
                .catalog()
                .iter()
                .chain(relation.schema())
                .map(String::as_str)
                .chain(std::iter::once(relation.name().as_str()))
                .collect::<Vec<_>>()
                .join(".");
            let column = access
                .column_name()
                .as_ref()
                .map_or(String::new(), |name| format!(".{name}"));
            writeln!(
                output,
                "    {relation_name}{column} ({}): {}",
                access.relation_kind().label(),
                access.access_kind().name()
            )?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod test_helpers;
#[cfg(test)]
mod test_response_display {
    #[test]
    fn displays_returned_column_access_and_unresolved_columns() -> ::testresult::TestResult {
        let response = super::test_helpers::column_access_response()?;
        let mut output = Vec::new();
        super::write_response(&mut output, "queries.yaml#$.queries[2]", &response)?;
        let output = String::from_utf8(output)?;
        assert!(output.contains("queries.yaml#$.queries[2] (statement 0)"));
        assert!(output.contains("Column access (1):"));
        assert!(output.contains("public.events.id (Table): SelectOutput"));
        assert!(output.contains("[0] id: unresolved; unknown"));
        assert!(output.contains("[1] ids: pg_catalog.int4[]; nullable"));
        assert!(output.find("[0]").unwrap() < output.find("[1]").unwrap());
        Ok(())
    }
}

fn type_name(reference: Option<&TypeReference>) -> String {
    match reference {
        Some(reference) => format!(
            "{}.{}{}",
            reference.schema_name(),
            reference.local_name(),
            if *reference.is_array() { "[]" } else { "" },
        ),
        None => "unresolved".to_owned(),
    }
}

#[cfg(test)]
mod test_type_display {
    use super::{TypeReference, type_name};

    #[rstest::rstest]
    #[case::scalar(Some(false), "pg_catalog.int4")]
    #[case::array(Some(true), "pg_catalog.int4[]")]
    #[case::unresolved(None, "unresolved")]
    fn distinguishes_unresolved_scalar_and_array_types(
        #[case] array: Option<bool>,
        #[case] expected: &str,
    ) {
        let reference = array.map(|array| TypeReference::new("pg_catalog", "int4", array));
        assert_eq!(type_name(reference.as_ref()), expected);
    }
}

// CLI presentation stays local without extending the protocol's public API.
trait AnalysisLabel {
    fn label(&self) -> &'static str;
}

impl AnalysisLabel for QueryKind {
    fn label(&self) -> &'static str {
        match self {
            Self::Select => "Select",
            Self::Insert => "Insert",
            Self::Update => "Update",
            Self::Delete => "Delete",
            Self::Truncate => "Truncate",
            Self::Merge => "Merge",
            Self::Other => "Other",
        }
    }
}

impl AnalysisLabel for ObjectKind {
    fn label(&self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Table => "Table",
            Self::View => "View",
            Self::Type => "Type",
            Self::Function => "Function",
            Self::MaterializedView => "MaterializedView",
            Self::Sequence => "Sequence",
            Self::Trigger => "Trigger",
            Self::Index => "Index",
        }
    }
}
