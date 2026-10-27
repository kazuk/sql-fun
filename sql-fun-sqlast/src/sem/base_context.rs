mod aggregate_info;
mod builtin_casts;
mod builtin_fun;
mod builtin_ope;
#[cfg(test)]
mod test_helpers;
pub use self::{
    builtin_casts::PgBuiltinCasts, builtin_fun::PgBuiltInFunctions, builtin_ope::PgBuiltinOperators,
};

use std::{fs::File, io::BufReader, path::Path};

use crate::context::BaseParseContext;
use crate::{
    TrieMap,
    context::{
        FunctionInfoRead, GetSourcePathWithSpanOpt, IndexInfoRead, OperatorInfoRead,
        SearchPathRead, SequenceInfoRead, TableInfoRead, TypeInfoRead, ViewInfoRead,
    },
    sem::{
        AnalysisError, CastDefinition, CreateTable, FullName, FunctionOverloadCollection,
        OperatorDefinition, SchemaName, SourcePathWithSpan, TypeDefinition, TypeReference,
        ViewLike,
    },
};
use ::sql_fun_core::{PostgresBuiltinPaths, SqlFunArgs, SqlFunMetadata};
use serde::de::DeserializeOwned;

/// Builtin schema information initialized from the selected metadata.
#[derive(Debug)]
pub struct BaseContext {
    configured_roles: crate::sem::ownership::ConfiguredRoles,
    context_args: SqlFunArgs,
    metadata: SqlFunMetadata,
    types: TrieMap<TypeDefinition>,
    binary_ordered_types: std::collections::HashSet<FullName>,
    casts: TrieMap<TrieMap<CastDefinition>>,
    binary_operators: TrieMap<OperatorDefinition>,
    left_unary_operators: TrieMap<OperatorDefinition>,
    right_unary_operators: TrieMap<OperatorDefinition>,
    search_path: Vec<SchemaName>,
    functions: PgBuiltInFunctions,
    masters: crate::sem::ConflictMasters,
}

#[derive(serde::Deserialize)]
struct BuiltinTypeInfo {
    schema: String,
    name: String,
    #[serde(default)]
    category: Option<crate::sem::TypeCategory>,
    #[serde(deserialize_with = "crate::sem::pg_type::deserialize_collatable")]
    collatable: Option<bool>,
}

#[cfg(test)]
mod builtin_type_info_tests {
    use super::BuiltinTypeInfo;

    #[rstest::rstest]
    #[case::missing_schema(r#"{"name":"text","collatable":true}"#)]
    #[case::null_schema(r#"{"schema":null,"name":"text","collatable":true}"#)]
    #[case::missing_fact(r#"{"schema":"pg_catalog","name":"text"}"#)]
    #[case::invalid_fact(r#"{"schema":"pg_catalog","name":"text","collatable":"true"}"#)]
    fn invalid_required_fields_fail(#[case] json: &str) {
        assert!(serde_json::from_str::<BuiltinTypeInfo>(json).is_err());
    }
}

impl From<&BuiltinTypeInfo> for TypeDefinition {
    fn from(info: &BuiltinTypeInfo) -> Self {
        let name = FullName::with_schema(&info.schema, &info.name);
        Self::builtin_with_category(&name, info.category, info.collatable)
    }
}

#[cfg(test)]
mod builtin_type_conversion_tests {
    use super::BuiltinTypeInfo;
    use crate::{
        context::BaseParseContext,
        sem::{FullName, TypeDefinition, TypeReference},
        test_helpers::TestParseContext,
    };

    #[rstest::rstest]
    #[case::yes("true", Some(true))]
    #[case::no("false", Some(false))]
    #[case::undefined("null", None)]
    fn retains_required_nullable_fact_and_qualified_identity(
        #[case] value: &str,
        #[case] expected: Option<bool>,
    ) {
        let json = format!(
            r#"{{"schema":"Odd.Schema","name":"_Type 雪","category":null,"collatable":{value}}}"#
        );
        let record: BuiltinTypeInfo = serde_json::from_str(&json).unwrap();
        let definition = TypeDefinition::from(&record);
        let reference = TypeReference::dynamic(&definition);
        let context = TestParseContext::default();
        assert_eq!(reference.collation_capability(&context), expected);
        assert_eq!(
            definition.full_name(),
            Some(&FullName::with_schema("Odd.Schema", "_Type 雪"))
        );
    }
    #[rstest::rstest]
    #[case::seventeen("17")]
    #[case::eighteen("18")]
    fn committed_catalogs_survive_loading_and_lookup(#[case] version: &str) {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../sql-fun-home/postgres")
            .join(version)
            .join("pg_type.json");
        let catalog = std::fs::read_to_string(path).unwrap();
        let records: Vec<BuiltinTypeInfo> = serde_json::from_str(&catalog).unwrap();
        assert!(!records.is_empty());
        let mut context = TestParseContext::default();
        for record in records {
            assert!(
                record.category.is_some(),
                "defined builtin: {}",
                record.name
            );
            let definition = TypeDefinition::from(&record);
            let name = FullName::with_schema(&record.schema, &record.name);
            context.set_get_type_impl_result(&name, &definition);
            for lookup in [name, FullName::from_local_name(&record.name)] {
                let loaded = context.get_type(&lookup).expect("type lookup");
                assert_eq!(loaded.category(&context), record.category);
                assert_eq!(
                    TypeReference::dynamic(loaded).collation_capability(&context),
                    record.collatable,
                );
                let json = serde_json::to_string(loaded).unwrap();
                let restored: TypeDefinition = serde_json::from_str(&json).unwrap();
                assert_eq!(restored, *loaded);
                assert_eq!(restored.category(&context), record.category);
            }
        }
    }

    #[rstest::rstest]
    #[case::shell(
        r#"{"schema":"pg_catalog","collatable":null,"name":"custom","category":null}"#,
        None
    )]
    #[case::custom(
        r#"{"schema":"pg_catalog","collatable":null,"name":"custom","category":"!"}"#,
        Some('!')
    )]
    #[case::unknown(
        r#"{"schema":"pg_catalog","collatable":null,"name":"unknown","category":"X"}"#,
        Some('X')
    )]
    fn preserves_available_and_missing_information(
        #[case] json: &str,
        #[case] expected: Option<char>,
    ) {
        let record: BuiltinTypeInfo = serde_json::from_str(json).unwrap();
        let definition = TypeDefinition::from(&record);
        let context = TestParseContext::default();
        assert_eq!(
            definition
                .category(&context)
                .map(crate::sem::TypeCategory::code),
            expected
        );
        let json = serde_json::to_string(&definition).unwrap();
        let restored: TypeDefinition = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.category(&context), definition.category(&context));
    }
}

impl BaseContext {
    /// Initialize builtin schema information with the caller's selected metadata.
    ///
    /// The metadata is retained for semantic analysis; this constructor does not
    /// reload the metadata file or select a datasource.
    ///
    /// # Errors
    ///
    /// Returns an error if builtin definitions or their configuration cannot be loaded.
    pub fn new(
        context_args: &SqlFunArgs,
        metadata: &SqlFunMetadata,
    ) -> Result<Self, AnalysisError> {
        let paths = PostgresBuiltinPaths::try_from_args(context_args, metadata)?;
        let builtin_functions = paths.functions()?;
        let function_details = paths.function_details()?;
        let builtin_type_data = Self::load_builtin_types(&paths)?;
        let search_path = Self::setup_search_path(context_args, metadata)?;
        let functions = PgBuiltInFunctions::load_from(builtin_functions, function_details)?;
        let binary_operators = PgBuiltinOperators::load_binary(paths.binary_operators()?)?;
        let left_unary_operators =
            PgBuiltinOperators::load_left_unary(paths.left_unary_operators()?)?;
        let right_unary_operators =
            PgBuiltinOperators::load_right_unary(paths.right_unary_operators()?)?;
        let masters = if crate::ast::Dialect::from(metadata) == crate::ast::Dialect::sqlite() {
            crate::sem::ConflictMasters::sqlite()
        } else {
            crate::sem::ConflictMasters::load(&paths.collations()?, &paths.operator_classes()?)?
        };

        let (casts, binary_ordered_types) = PgBuiltinCasts::load(paths.casts()?, &masters)?;
        Ok(Self {
            configured_roles: metadata.into(),
            context_args: context_args.clone(),
            metadata: metadata.clone(),
            functions,
            masters,
            types: builtin_type_data,
            binary_ordered_types,
            casts,
            binary_operators,
            left_unary_operators,
            right_unary_operators,
            search_path,
        })
    }

    /// Declare a connection-local SQLite collation without opening a connection.
    ///
    /// # Errors
    /// Returns an error for a non-SQLite context or an invalid name.
    pub fn declare_sqlite_collation(&mut self, name: &str) -> Result<(), AnalysisError> {
        self.masters.declare_sqlite_collation(name)
    }

    fn load_builtin_types(
        paths: &PostgresBuiltinPaths,
    ) -> Result<TrieMap<TypeDefinition>, AnalysisError> {
        Self::read_builtin_types(&paths.types()?)
    }

    fn read_builtin_types(
        builtin_types_path: &Path,
    ) -> Result<TrieMap<TypeDefinition>, AnalysisError> {
        let builtin_types: Vec<BuiltinTypeInfo> = serde_json::from_reader(BufReader::new(
            File::open(builtin_types_path)?,
        ))
        .map_err(|error| {
            AnalysisError::unexpected_input_error(&format!(
                "invalid type catalog {}: {error}",
                builtin_types_path.display()
            ))
        })?;
        let mut builtin_type_data = TrieMap::new();
        for builtin_type in builtin_types {
            if builtin_type.schema.is_empty()
                || builtin_type.name.is_empty()
                || builtin_type.schema.contains('\0')
                || builtin_type.name.contains('\0')
            {
                return Err(AnalysisError::unexpected_input_error(&format!(
                    "invalid type identity in {}",
                    builtin_types_path.display()
                )));
            }
            let type_definition = TypeDefinition::from(&builtin_type);
            let full_name = type_definition.full_name().expect("catalog type identity");
            if builtin_type_data.get(&full_name.to_string()).is_some() {
                return Err(AnalysisError::unexpected_input_error(&format!(
                    "duplicate type {full_name} in {}",
                    builtin_types_path.display()
                )));
            }
            builtin_type_data.insert(&full_name.to_string(), type_definition);
        }
        Ok(builtin_type_data)
    }
}

#[cfg(test)]
mod read_builtin_types_tests {
    use super::{BaseContext, test_helpers};
    use crate::{
        sem::{FullName, TypeReference},
        test_helpers::TestParseContext,
    };

    #[rstest::rstest]
    #[case::empty_schema(r#"[{"schema":"","name":"text","collatable":true}]"#)]
    #[case::empty_name(r#"[{"schema":"public","name":"","collatable":true}]"#)]
    #[case::nul_name(r#"[{"schema":"public","name":"a\u0000b","collatable":true}]"#)]
    #[case::duplicate(r#"[{"schema":"public","name":"custom","collatable":true},{"schema":"public","name":"custom","collatable":false}]"#)]
    fn invalid_catalog_reports_its_path(#[case] json: &str) {
        let fixture = test_helpers::TypeCatalog::new(json);
        let error = BaseContext::read_builtin_types(fixture.path()).unwrap_err();
        assert!(error.to_string().contains(fixture.path().to_str().unwrap()));
    }

    #[test]
    fn same_local_names_retain_separate_qualified_facts() {
        let fixture = test_helpers::TypeCatalog::new(
            r#"[
            {"schema":"one","name":"custom","collatable":true},
            {"schema":"two","name":"custom","collatable":false}
        ]"#,
        );
        let types = BaseContext::read_builtin_types(fixture.path()).unwrap();
        let context = TestParseContext::default();
        for (schema, expected) in [("one", true), ("two", false)] {
            let name = FullName::with_schema(schema, "custom");
            let loaded = types.get(&name.to_string()).unwrap();
            assert_eq!(
                TypeReference::dynamic(loaded).collation_capability(&context),
                Some(expected)
            );
        }
    }
}

impl BaseContext {
    fn setup_search_path(
        context_args: &SqlFunArgs,
        metadata: &SqlFunMetadata,
    ) -> Result<Vec<SchemaName>, AnalysisError> {
        let search_path = context_args
            .postgres_search_path(metadata)?
            .into_iter()
            .map(|v| SchemaName::from(&v))
            .collect();
        Ok(search_path)
    }
}

#[cfg(test)]
mod test_metadata;

impl TypeInfoRead for BaseContext {
    fn type_references(&self) -> Vec<TypeReference> {
        self.types
            .iter()
            .filter_map(|(_, definition)| {
                definition
                    .full_name()
                    .map(|name| TypeReference::concrete_type_ref(name, false))
            })
            .collect()
    }

    fn builtin_type_ordered(&self, name: &FullName) -> Option<bool> {
        if crate::ast::Dialect::from(&self.metadata) != crate::ast::Dialect::postgres() {
            return None;
        }
        self.types.get(&name.to_string())?;
        Some(self.masters.type_ordered(name) || self.binary_ordered_types.contains(name))
    }
    fn get_type_impl(&self, key: &FullName) -> Option<&TypeDefinition> {
        tracing::info!("get type {key}");
        self.types.get(&key.to_string())
    }

    fn get_array_item_type(&self, type_name: &TypeReference) -> Option<TypeReference> {
        if type_name.is_array() == Some(true) {
            Some(type_name.element_type())
        } else {
            if crate::ast::Dialect::from(&self.metadata) != crate::ast::Dialect::postgres() {
                return None;
            }
            let (name, _) = self.builtin_cast_type(type_name)?;
            // pg_type.json omits typelem for these PostgreSQL catalog vectors.
            let element = match name {
                "int2vector" => "int2",
                "oidvector" => "oid",
                _ => return None,
            };
            Some(TypeReference::from_master_type_name(element))
        }
    }
}

impl TableInfoRead for BaseContext {
    fn get_table_impl(&self, _key: &FullName) -> Option<&CreateTable> {
        None
    }
}

impl ViewInfoRead for BaseContext {
    fn get_view_impl(&self, _key: &FullName) -> Option<&ViewLike> {
        None
    }
}

impl IndexInfoRead for BaseContext {
    fn indexes_for_table(&self, _table: &crate::sem::TableName) -> Vec<&super::CreateIndex> {
        Vec::new()
    }
    fn get_index_impl(&self, _index_name: &FullName) -> Option<&super::CreateIndex> {
        // Builtin metadata contains no index definitions.
        None
    }
}

impl SequenceInfoRead for BaseContext {
    fn get_sequence_impl(&self, _seq_name: &FullName) -> Option<&super::CreateSequence> {
        None
    }
}

impl FunctionInfoRead for BaseContext {
    fn is_aggregate_impl(&self, name: &FullName) -> bool {
        self::aggregate_info::builtin_aggregate(name)
    }
    fn function_preserves_view_rows(
        &self,
        name: &FullName,
        overload: &crate::sem::OverloadVariant,
    ) -> bool {
        self.functions.preserves_view_rows(name, overload)
    }

    fn get_function_impl(&self, name: &FullName) -> Option<FunctionOverloadCollection> {
        let create_function = self.functions.find_function_by_name(name)?;
        Some(create_function.overloads().clone())
    }
}

impl OperatorInfoRead for BaseContext {
    fn operator_result_is_not_null(&self, name: &super::OperatorName) -> Option<bool> {
        // The shipped equality implementations return a boolean for non-null
        // operands. This guarantee does not extend to arbitrary SQL functions.
        if *name == super::OperatorName::from_parts(None, "=")
            && self.get_operator_op(name, true, true).is_some()
        {
            Some(true)
        } else {
            None
        }
    }

    #[tracing::instrument(skip(self))]
    fn get_operator_op(
        &self,
        name: &super::OperatorName,
        has_left: bool,
        has_right: bool,
    ) -> Option<&super::OperatorDefinition> {
        if !name.is_valid() {
            return None;
        }

        match (has_left, has_right) {
            (true, true) => self.binary_operators.get(&name.to_name_string()),
            (true, false) => self.left_unary_operators.get(&name.to_name_string()),
            (false, true) => self.right_unary_operators.get(&name.to_name_string()),
            _ => panic!("no-param operator lookup"),
        }
    }
}

impl SearchPathRead for BaseContext {
    fn get_default_schema(&self) -> Option<SchemaName> {
        self.search_path.first().cloned()
    }

    fn get_search_path(&self) -> &[SchemaName] {
        &self.search_path
    }
}

impl GetSourcePathWithSpanOpt for BaseContext {
    fn get_source_path_with_span(&self) -> Option<&SourcePathWithSpan> {
        None
    }
}

impl crate::context::TriggerInfoRead for BaseContext {
    fn trigger(
        &self,
        _identity: &crate::sem::TriggerIdentity,
    ) -> Option<&crate::sem::CreateTrigger> {
        None
    }
}

impl BaseParseContext for BaseContext {
    fn sqlite_relation_identity(&self, _name: &FullName) -> Option<FullName> {
        None
    }

    fn metadata(&self) -> &SqlFunMetadata {
        &self.metadata
    }

    fn get_args(&self) -> &SqlFunArgs {
        &self.context_args
    }

    fn get_source_file_path(&self) -> Option<&Path> {
        None // BaseContext contains built-in data item, from Non SQL Source
    }
}

fn deserialize_from_path<P, T>(path: P) -> Result<T, AnalysisError>
where
    P: AsRef<Path>,
    T: DeserializeOwned,
{
    Ok(serde_json::from_reader(BufReader::new(File::open(path)?))?)
}

impl crate::context::ConflictInfoRead for BaseContext {
    fn database_collation(&self) -> Option<&crate::sem::DatabaseCollation> {
        self.masters.database()
    }
    fn collation_definition(&self, name: &FullName) -> Option<&crate::sem::CollationDefinition> {
        self.masters.qualified_collation(name)
    }
    fn operator_class_definition(
        &self,
        name: &FullName,
        method: &str,
    ) -> Option<&crate::sem::OperatorClassDefinition> {
        self.masters.operator_class(name, method)
    }
    fn lookup_sqlite_collation(&self, name: &FullName) -> crate::sem::CatalogNameLookup {
        self.masters.sqlite_lookup(name)
    }
    fn lookup_collation(&self, name: &FullName) -> crate::sem::CatalogNameLookup {
        crate::sem::conflict_masters::lookup_collation(self, name)
    }
    fn lookup_operator_class(
        &self,
        name: &FullName,
        access_method: &str,
    ) -> crate::sem::CatalogNameLookup {
        crate::sem::conflict_masters::lookup_operator_class(self, name, access_method)
    }
    fn infer_conflict_arbiters(
        &self,
        table: &crate::sem::TableName,
        conflict: &crate::sem::InsertConflict,
    ) -> Result<crate::sem::ConflictArbiterInference, AnalysisError> {
        crate::sem::conflict_info::infer_from_catalog(self, table, conflict)
    }
}

impl crate::context::ViewInsertInfoRead for BaseContext {
    fn view_insert_support(&self, _view: &FullName) -> Option<&crate::sem::ViewInsertSupport> {
        None
    }
}

impl crate::context::ViewUpdateInfoRead for BaseContext {
    fn view_update_support(
        &self,
        _view: &crate::sem::FullName,
    ) -> Option<&crate::sem::ViewUpdateSupport> {
        None
    }
}

impl crate::context::ViewDeleteInfoRead for BaseContext {
    fn view_delete_support(&self, _view: &crate::sem::FullName) -> crate::sem::ViewDeleteSupport {
        todo!("catalog deletable views, INSTEAD OF DELETE triggers and rules")
    }
}

impl crate::context::ViewMergeInfoRead for BaseContext {
    fn view_merge_support(&self, _view: &crate::sem::FullName) -> crate::sem::ViewMergeSupport {
        todo!("catalog MERGE-eligible views, rules, triggers and action columns")
    }
}

impl crate::context::RoleInfoRead for BaseContext {
    fn registered_role_by_name(&self, _name: &str) -> Option<&crate::sem::Role> {
        None
    }

    fn role_by_name(&self, name: &str) -> Option<&crate::sem::Role> {
        self.configured_roles.role(name)
    }
}
impl crate::context::OwnershipInfoRead for BaseContext {
    fn routine_definitions(&self, name: &FullName) -> Vec<crate::sem::CreateFunction> {
        self.get_function_impl(name)
            .map_or_else(Vec::new, |overloads| {
                crate::sem::CreateFunction::new(name, &overloads, &None).split_definitions()
            })
    }
    fn ownership_definitions(
        &self,
        kind: crate::sem::ObjectKind,
        name: &FullName,
    ) -> Vec<crate::sem::OwnedObjectDefinition> {
        crate::sem::ownership::model_definitions(self, kind, name)
    }
}

impl crate::context::ObjectInfoRead for BaseContext {
    fn object_kind(&self, _: &FullName) -> Option<crate::sem::Kind> {
        None
    }
}
impl crate::context::ExtensionInfoRead for BaseContext {
    fn extension_exists(&self, _: &str) -> bool {
        false
    }
}
impl crate::context::RuleInfoRead for BaseContext {}
