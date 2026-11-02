use crate::check::submission::QueryAnalysis;
use ::anyhow::Context;
use ::sql_fun_client::payloads::{AnalyzeQueryResponse, ColumnAccess};
use ::sql_fun_codegen::QueryRequirement;
use ::sql_fun_core::{Hasher, SqlFunArgs};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Write,
    path::{Path, PathBuf},
};

/// Source identity is independent of Rust generation and function-name collisions.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct QueryIdentity {
    function: String,
    source_file: String,
    source_line: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RelationIdentity {
    kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    catalog: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    schema: Option<String>,
    name: String,
}

impl TryFrom<&ColumnAccess> for RelationIdentity {
    type Error = ::anyhow::Error;

    fn try_from(access: &ColumnAccess) -> Result<Self, Self::Error> {
        Ok(Self {
            kind: serialized_name(access.relation_kind())?,
            catalog: access.relation().catalog().clone(),
            schema: access.relation().schema().clone(),
            name: access.relation().name().clone(),
        })
    }
}

fn serialized_name(value: &impl ::serde::Serialize) -> Result<String, ::anyhow::Error> {
    let value = ::serde_json::to_value(value)?;
    Ok(value
        .as_str()
        .context("access kind must serialize as a name")?
        .to_owned())
}

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd)]
struct Access {
    relation: RelationIdentity,
    column: Option<String>,
    kind: String,
    statement: usize,
}

struct QueryFacts {
    fingerprint: String,
    statements: usize,
    accesses: BTreeSet<Access>,
}

impl QueryFacts {
    fn collect(
        fingerprint: &str,
        response: &AnalyzeQueryResponse,
    ) -> Result<Self, ::anyhow::Error> {
        let mut accesses = BTreeSet::new();
        for (index, statement) in response.queries().items().iter().enumerate() {
            for access in statement.column_access().items() {
                accesses.insert(Access {
                    relation: RelationIdentity::try_from(access)?,
                    column: access.column_name().clone(),
                    kind: serialized_name(access.access_kind())?,
                    statement: index + 1,
                });
            }
        }
        Ok(Self {
            fingerprint: fingerprint.to_owned(),
            statements: response.queries().items().len(),
            accesses,
        })
    }
}

#[cfg(test)]
mod test_query_facts {
    use super::QueryFacts;
    use crate::test_helpers::inventory::access_response;

    #[rstest::rstest]
    fn ignores_generation_eligibility_and_diagnostics(#[values(false, true)] eligible: bool) {
        let facts = QueryFacts::collect("hash", &access_response(eligible)).unwrap();
        assert_eq!(facts.statements, 2);
        // Duplicate reads collapse; distinct roles and statement indices survive.
        assert_eq!(facts.accesses.len(), 5);
        assert_eq!(
            facts.accesses,
            QueryFacts::collect("hash", &access_response(!eligible))
                .unwrap()
                .accesses
        );
    }
    #[rstest::rstest]
    fn preserves_sql_visible_object_kinds(#[values("View", "Function")] kind: &str) {
        let facts = QueryFacts::collect(
            "hash",
            &crate::test_helpers::inventory::object_response(kind),
        )
        .unwrap();
        assert!(
            facts
                .accesses
                .iter()
                .all(|access| access.relation.kind == kind
                    && access.relation.name == "visible_object")
        );
    }

    #[test]
    fn accepts_requested_zero_access_statements() {
        let response = ::sql_fun_client::payloads::AnalyzeQueryResponse::new(
            ::sql_fun_client::Collection::completed(Vec::new()),
        );
        let facts = QueryFacts::collect("hash", &response).unwrap();
        assert!(facts.accesses.is_empty());
    }
}

#[derive(Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct QueryRecord {
    id: usize,
    #[serde(flatten)]
    source: QueryIdentity,
    fingerprint: String,
    statements: usize,
}

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Reference {
    kind: String,
    query: usize,
    statement: usize,
}

#[derive(Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ColumnRecord {
    name: String,
    accesses: BTreeSet<Reference>,
}

#[derive(Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RelationRecord {
    #[serde(flatten)]
    identity: RelationIdentity,
    accesses: BTreeSet<Reference>,
    columns: Vec<ColumnRecord>,
}

#[derive(Default)]
struct RelationFacts {
    accesses: BTreeSet<Reference>,
    columns: BTreeMap<String, BTreeSet<Reference>>,
}

impl RelationFacts {
    fn receive(&mut self, access: Access, query: usize) {
        let reference = Reference {
            kind: access.kind,
            query,
            statement: access.statement,
        };
        if let Some(column) = access.column {
            self.columns.entry(column).or_default().insert(reference);
        } else {
            self.accesses.insert(reference);
        }
    }

    fn into_record(self, identity: RelationIdentity) -> RelationRecord {
        RelationRecord {
            identity,
            accesses: self.accesses,
            columns: self
                .columns
                .into_iter()
                .map(|(name, accesses)| ColumnRecord { name, accesses })
                .collect(),
        }
    }
}

#[derive(Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    inventory_version: u32,
    datasource: String,
    schema_variant: String,
    schema_dump_hash: String,
    queries: Vec<QueryRecord>,
    relations: Vec<RelationRecord>,
}

impl Document {
    fn build(target: Target) -> Self {
        let mut queries = Vec::new();
        let mut relations = BTreeMap::<RelationIdentity, RelationFacts>::new();
        for (index, (source, facts)) in target.queries.into_iter().enumerate() {
            let id = index + 1;
            queries.push(QueryRecord {
                id,
                source,
                fingerprint: facts.fingerprint,
                statements: facts.statements,
            });
            for access in facts.accesses {
                relations
                    .entry(access.relation.clone())
                    .or_default()
                    .receive(access, id);
            }
        }
        Self {
            inventory_version: 1,
            datasource: target.datasource,
            schema_variant: target.schema_variant,
            schema_dump_hash: target.schema_hash,
            queries,
            relations: relations
                .into_iter()
                .map(|(identity, facts)| facts.into_record(identity))
                .collect(),
        }
    }

    fn load(path: &Path) -> Result<Option<Self>, ::anyhow::Error> {
        let content = match ::fs_err::read_to_string(path) {
            Ok(content) => content,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let document: Self = ::toml::from_str(&content)
            .with_context(|| format!("invalid inventory {}", path.display()))?;
        ::anyhow::ensure!(
            document.inventory_version == 1,
            "unsupported inventory version in {}",
            path.display()
        );
        Ok(Some(document))
    }
}

#[cfg(test)]
mod test_document {
    use super::{Document, QueryFacts, QueryIdentity, Target};
    use crate::test_helpers::inventory::{Project, access_response};

    #[test]
    fn preserves_composite_identities_roles_and_quoted_names() {
        let project = Project::new();
        let mut target = Target::new(&project.args(), "first", false, false).unwrap();
        for source_line in [20, 10] {
            target.queries.insert(
                QueryIdentity {
                    function: "same_name".to_owned(),
                    source_file: "src/query.rs".to_owned(),
                    source_line,
                },
                QueryFacts::collect("hash", &access_response(false)).unwrap(),
            );
        }
        let document = Document::build(target);
        assert_eq!(document.queries.len(), 2);
        assert_eq!(document.queries[0].source.source_line, 10);
        assert_eq!(document.relations.len(), 1);
        assert_eq!(document.relations[0].accesses.len(), 4);
        assert_eq!(document.relations[0].columns[0].accesses.len(), 6);
        let encoded = ::toml::to_string_pretty(&document).unwrap();
        let restored: Document = ::toml::from_str(&encoded).unwrap();
        assert_eq!(restored, document);
        assert_eq!(::toml::to_string_pretty(&restored).unwrap(), encoded);
    }
}

struct Target {
    path: PathBuf,
    previous: Option<Document>,
    datasource: String,
    schema_variant: String,
    schema_hash: String,
    queries: BTreeMap<QueryIdentity, QueryFacts>,
}

impl Target {
    fn new(
        args: &SqlFunArgs,
        datasource: &str,
        force: bool,
        expect_schema: bool,
    ) -> Result<Self, ::anyhow::Error> {
        let selected = args.with_datasource(datasource);
        let schema_variant = selected.schema();
        let path = selected
            .project_dir()?
            .join(format!("sql-fun.{datasource}.{schema_variant}.toml"));
        let schema_hash = Hasher::hash_file(selected.schema_file(&selected.metadata_file()?)?)?;
        let previous = if force { None } else { Document::load(&path)? };
        if expect_schema && let Some(previous) = &previous {
            ::anyhow::ensure!(
                previous.schema_dump_hash == schema_hash,
                "inventory schema dump changed: {}",
                path.display()
            );
        }
        Ok(Self {
            path,
            previous,
            datasource: datasource.to_owned(),
            schema_variant: schema_variant.to_owned(),
            schema_hash,
            queries: BTreeMap::new(),
        })
    }

    fn receive(&mut self, analysis: &QueryAnalysis) -> Result<(), ::anyhow::Error> {
        let identity = QueryIdentity {
            function: analysis.function_name().to_owned(),
            source_file: analysis.source_file().to_owned(),
            source_line: analysis.source_line(),
        };
        let facts = QueryFacts::collect(analysis.query_finger_print(), analysis.response())?;
        self.queries.insert(identity, facts);
        Ok(())
    }
}

/// Collect only returned access facts; never classify SQL diagnostics or generation eligibility.
pub(super) struct Inventories(BTreeMap<String, Target>);

impl Inventories {
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
                    Target::new(args, datasource, force, expect_schema)?,
                );
            }
        }
        Ok(Self(targets))
    }

    pub(super) fn receive(&mut self, analysis: &QueryAnalysis) -> Result<(), ::anyhow::Error> {
        self.0
            .get_mut(analysis.datasource())
            .context("analysis returned an unexpected inventory datasource")?
            .receive(analysis)
    }

    /// Prepare every document before replacing any file. No writes in validation mode.
    pub(super) fn finalize(self, validate_only: bool) -> Result<bool, ::anyhow::Error> {
        let mut prepared = Vec::new();
        let mut matches = true;
        for (_, mut target) in self.0 {
            let path = target.path.clone();
            let previous = target.previous.take();
            let document = Document::build(target);
            if previous.as_ref() != Some(&document) {
                matches = false;
                if validate_only {
                    ::log::error!("inventory validation mismatch: {}", path.display());
                }
            }
            prepared.push((path, ::toml::to_string_pretty(&document)?));
        }
        if !validate_only {
            for (path, content) in prepared {
                replace(&path, &content)?;
            }
        }
        Ok(!validate_only || matches)
    }
}

#[cfg(test)]
mod test_inventories {
    use super::{Inventories, Target};
    use crate::test_helpers::inventory::Project;
    use std::collections::BTreeMap;

    #[test]
    fn validates_without_writes_and_preserves_untouched_datasources() {
        let project = Project::new();
        let args = project.args();
        project.seed_inventory("second", "unrelated inventory");
        let target = Target::new(&args, "first", false, false).unwrap();
        let inventories = Inventories(BTreeMap::from([("first".to_owned(), target)]));
        assert!(!inventories.finalize(true).unwrap());
        assert!(project.inventory_bytes("first").is_none());
        let target = Target::new(&args, "first", false, false).unwrap();
        Inventories(BTreeMap::from([("first".to_owned(), target)]))
            .finalize(false)
            .unwrap();
        let before = project.inventory_bytes("first").unwrap();
        let target = Target::new(&args, "first", false, false).unwrap();
        assert!(
            Inventories(BTreeMap::from([("first".to_owned(), target)]))
                .finalize(true)
                .unwrap()
        );
        assert_eq!(project.inventory_bytes("first").unwrap(), before);
        assert_eq!(
            project.inventory_bytes("second").unwrap(),
            b"unrelated inventory"
        );
    }

    #[test]
    fn detects_header_changes_and_respects_schema_guard() {
        let project = Project::new();
        let args = project.args();
        let mut target = Target::new(&args, "first", false, false).unwrap();
        target.schema_hash = "old hash".to_owned();
        Inventories(BTreeMap::from([("first".to_owned(), target)]))
            .finalize(false)
            .unwrap();
        assert!(Target::new(&args, "first", false, true).is_err());
        let target = Target::new(&args, "first", false, false).unwrap();
        assert!(
            !Inventories(BTreeMap::from([("first".to_owned(), target)]))
                .finalize(true)
                .unwrap()
        );
    }
}

fn replace(path: &Path, content: &str) -> Result<(), ::anyhow::Error> {
    let mut temporary =
        ::tempfile::NamedTempFile::new_in(path.parent().context("inventory path has no parent")?)?;
    temporary.write_all(content.as_bytes())?;
    temporary
        .persist(path)
        .with_context(|| format!("cannot replace inventory {}", path.display()))?;
    Ok(())
}
