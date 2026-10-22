#[cfg(test)]
mod test_helpers;

use super::submission::QueryAnalysis;
use ::anyhow::Context;
use ::sql_fun_client::payloads::{AnalyzeQueryItem, TypeReference};
use ::sql_fun_codegen::QueryRequirement;
use ::sql_fun_core::{
    Hasher, SqlFunArgs,
    lockfile::{
        LockfileDocument, LockfileEntry, LockfileStatement, QueryParameter, ResultSetField,
    },
};
use std::{collections::BTreeMap, path::PathBuf};

struct Target {
    path: PathBuf,
    previous: Option<LockfileDocument>,
    prospective: LockfileDocument,
}

impl Target {
    fn new(
        args: &SqlFunArgs,
        datasource: &str,
        force: bool,
        expect_schema: bool,
    ) -> Result<Self, ::anyhow::Error> {
        let selected = args.with_datasource(datasource);
        let path = selected.lockfile_path(datasource)?;
        let hash = Hasher::hash_file(selected.schema_file(&selected.metadata_file()?)?)?;
        let previous = if force {
            None
        } else {
            LockfileDocument::load(&path)?
        };
        if expect_schema && let Some(existing) = &previous {
            existing.validate_schema(&path, &hash)?;
        }
        Ok(Self {
            path,
            previous,
            prospective: LockfileDocument::new(&hash),
        })
    }
}

/// A complete check session. It never writes while results are arriving.
pub(super) struct Lockfiles {
    targets: BTreeMap<String, Target>,
    analysis_failed: bool,
}

impl Lockfiles {
    pub(super) fn new(
        args: &SqlFunArgs,
        requirements: &[QueryRequirement],
        force: bool,
        expect_schema: bool,
    ) -> Result<Self, ::anyhow::Error> {
        let mut targets = BTreeMap::new();
        for requirement in requirements {
            let datasource = requirement.datasource();
            if !targets.contains_key(datasource) {
                targets.insert(
                    datasource.to_owned(),
                    Target::new(args, datasource, force, expect_schema).with_context(|| {
                        format!("cannot load lockfile for datasource {datasource}")
                    })?,
                );
            }
        }
        Ok(Self {
            targets,
            analysis_failed: false,
        })
    }

    pub(super) fn receive(&mut self, analysis: &QueryAnalysis) -> Result<(), ::anyhow::Error> {
        let entry = entry_from_analysis(analysis).with_context(|| {
            format!(
                "cannot encode {} at {}:{} (datasource {})",
                analysis.function_name(),
                analysis.source_file(),
                analysis.source_line(),
                analysis.datasource()
            )
        })?;
        let target = self
            .targets
            .get_mut(analysis.datasource())
            .context("analysis returned an unexpected datasource")?;
        if let Some(entry) = entry {
            target.prospective.push(entry);
        } else {
            self.analysis_failed = true;
        }
        Ok(())
    }

    /// Called only after producer AND connection completion have succeeded.
    pub(super) fn finalize(mut self, validate_only: bool) -> Result<bool, ::anyhow::Error> {
        let mut prepared = Vec::new();
        let mut matches = true;
        for target in self.targets.values_mut() {
            target.prospective.sort();
            if !target
                .prospective
                .matches_existing(target.previous.as_ref())
            {
                matches = false;
                if validate_only {
                    ::log::error!("lockfile validation mismatch: {}", target.path.display());
                }
            }
            prepared.push((&target.path, target.prospective.encode()?));
        }
        if !validate_only {
            for (path, content) in prepared {
                LockfileDocument::replace(path, &content)?;
            }
        }
        Ok(!self.analysis_failed && (!validate_only || matches))
    }
}

fn entry_from_analysis(analysis: &QueryAnalysis) -> Result<Option<LockfileEntry>, ::anyhow::Error> {
    let mut statements = Vec::new();
    for item in analysis.response().queries().items() {
        let Some(statement) = statement_from_item(item)? else {
            return Ok(None);
        };
        statements.push(statement);
    }
    if statements.is_empty() {
        return Ok(None);
    }
    Ok(Some(LockfileEntry::new(
        analysis.query_finger_print(),
        analysis.function_name(),
        &statements,
    )?))
}

fn statement_from_item(
    item: &AnalyzeQueryItem,
) -> Result<Option<LockfileStatement>, ::anyhow::Error> {
    if !item.complete() {
        return Ok(None);
    }
    let Some(sql) = item.execute_sql() else {
        return Ok(None);
    };
    let mut parameters = Vec::new();
    for parameter in item.parameters().items() {
        let index = u16::try_from(*parameter.parameter_index())
            .context("parameter index exceeds lockfile u16 range")?;
        ::anyhow::ensure!(index != 0, "parameter indices must be one-based");
        if parameter.parameter_type().is_none() {
            return Ok(None);
        }
        parameters.push(QueryParameter::new(parameter.name(), index));
    }
    parameters.sort_by_key(QueryParameter::parameter_index);
    let mut columns = Vec::new();
    for column in item.result_columns().items() {
        let index = u16::try_from(*column.ordinal())
            .context("result column ordinal exceeds lockfile u16 range")?;
        let Some(sql_type) = column.column_type() else {
            return Ok(None);
        };
        columns.push(ResultSetField::new(
            index,
            column.name(),
            &SqlTypeName::from(sql_type).0,
        ));
    }
    columns.sort_by_key(ResultSetField::field_index);
    Ok(Some(LockfileStatement::new(sql, &parameters, &columns)))
}

struct SqlTypeName(String);

impl From<&TypeReference> for SqlTypeName {
    fn from(value: &TypeReference) -> Self {
        let schema = value.schema_name().replace('"', "\"\"");
        let name = value.local_name().replace('"', "\"\"");
        let suffix = if *value.is_array() { "[]" } else { "" };
        Self(format!("\"{schema}\".\"{name}\"{suffix}"))
    }
}

#[cfg(test)]
mod test_statement {
    use super::{SqlTypeName, statement_from_item, test_helpers::statement};
    use ::sql_fun_client::payloads::TypeReference;

    #[test]
    fn preserves_binding_identity_and_column_order() {
        let item = statement(
            true,
            &[(3, "third"), (1, "first")],
            &[
                (1, Some(TypeReference::new("custom", "Thing", true))),
                (0, Some(TypeReference::new("pg_catalog", "int4", false))),
            ],
        );
        let mapped = statement_from_item(&item).unwrap().unwrap();
        assert_eq!(mapped.bind_parameters()[0].name(), "first");
        assert_eq!(mapped.bind_parameters()[0].parameter_index(), 1);
        assert_eq!(mapped.bind_parameters()[1].parameter_index(), 3);
        assert_eq!(mapped.result_set_shape()[0].field_index(), 0);
        let encoded = ::serde_json::to_value(mapped).unwrap();
        assert_eq!(
            encoded["result_set_shape"][1]["sql_type"],
            "\"custom\".\"Thing\"[]"
        );
    }

    #[rstest::rstest]
    #[case::incomplete(false, false)]
    #[case::unknown_type(true, true)]
    fn omits_unavailable_shapes(#[case] complete: bool, #[case] unknown: bool) {
        let columns = if unknown { vec![(0, None)] } else { Vec::new() };
        assert!(
            statement_from_item(&statement(complete, &[], &columns))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn accepts_nonreturning_statement() {
        assert!(
            statement_from_item(&statement(true, &[], &[]))
                .unwrap()
                .is_some()
        );
    }

    #[rstest::rstest]
    #[case::maximum(u32::from(u16::MAX), true)]
    #[case::overflow(u32::from(u16::MAX) + 1, false)]
    #[case::zero(0, false)]
    fn checked_parameter_indices(#[case] index: u32, #[case] valid: bool) {
        assert_eq!(
            statement_from_item(&statement(true, &[(index, "p")], &[])).is_ok(),
            valid
        );
    }

    #[rstest::rstest]
    #[case::maximum(u32::from(u16::MAX), true)]
    #[case::overflow(u32::from(u16::MAX) + 1, false)]
    fn checked_column_ordinals(#[case] ordinal: u32, #[case] valid: bool) {
        let item = statement(
            true,
            &[],
            &[(
                ordinal,
                Some(TypeReference::new("pg_catalog", "int4", false)),
            )],
        );
        assert_eq!(statement_from_item(&item).is_ok(), valid);
    }

    #[test]
    fn quotes_sql_type_identifiers_without_losing_array_identity() {
        assert_eq!(
            SqlTypeName::from(&TypeReference::new("odd\"schema", "a.b", true)).0,
            "\"odd\"\"schema\".\"a.b\"[]"
        );
    }
}
