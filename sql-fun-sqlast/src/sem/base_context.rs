mod aggregate_info;
mod builtin_casts;
mod builtin_fun;
mod builtin_ope;
pub use self::{
    builtin_casts::PgBuiltinCasts, builtin_fun::PgBuiltInFunctions, builtin_ope::PgBuiltinOperators,
};

use std::{fs::File, io::BufReader, path::Path};

use crate::{
    TrieMap,
    sem::{
        AnalysisError, BaseParseContext, CastDefinition, CreateTable, FullName, FunctionInfoRead,
        FunctionOverloadCollection, GetSourcePathWithSpanOpt, IndexInfoRead, LocalName,
        OperatorDefinition, SchemaName, SourcePathWithSpan, TypeDefinition, TypeReference,
        ViewLike,
        parse_context::{
            OperatorInfoRead, SearchPathRead, SequenceInfoRead, TableInfoRead, TypeInfoRead,
            ViewInfoRead,
        },
    },
};
use ::sql_fun_core::{PostgresBuiltinPaths, SqlFunArgs, SqlFunMetadata};
use serde::de::DeserializeOwned;

/// Builtin schema information initialized from the selected metadata.
#[derive(Debug)]
pub struct BaseContext {
    context_args: SqlFunArgs,
    metadata: SqlFunMetadata,
    types: TrieMap<TypeDefinition>,
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
    name: String,
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
        let casts = PgBuiltinCasts::load(paths.casts()?)?;
        let masters = if crate::ast::Dialect::from(metadata) == crate::ast::Dialect::sqlite() {
            crate::sem::ConflictMasters::sqlite()
        } else {
            crate::sem::ConflictMasters::load(&paths.collations()?, &paths.operator_classes()?)?
        };

        Ok(Self {
            context_args: context_args.clone(),
            metadata: metadata.clone(),
            functions,
            masters,
            types: builtin_type_data,
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
        let builtin_types_path = paths.types()?;
        let builtin_types: Vec<BuiltinTypeInfo> =
            serde_json::from_reader(BufReader::new(File::open(builtin_types_path)?))?;
        let mut builtin_type_data = TrieMap::new();
        for builtin_type in builtin_types {
            let full_name = FullName::new(
                &None,
                &Some(SchemaName::from("pg_catalog")),
                &LocalName::from(&builtin_type.name),
            );
            let type_definition = TypeDefinition::builtin(&full_name);
            builtin_type_data.insert(&full_name.to_string(), type_definition);
        }
        Ok(builtin_type_data)
    }

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
    fn get_type_impl(&self, key: &FullName) -> Option<&TypeDefinition> {
        tracing::info!("get type {key}");
        self.types.get(&key.to_string())
    }

    fn get_array_item_type(&self, type_name: &TypeReference) -> Option<TypeReference> {
        if type_name.is_array() == Some(true) {
            Some(type_name.element_type())
        } else {
            None
        }
    }
}

impl TableInfoRead for BaseContext {
    fn get_table_impl(&self, _key: &FullName) -> Option<&CreateTable> {
        None
    }
}

impl ViewInfoRead for BaseContext {
    fn view_trigger_target(&self, _trigger: &FullName) -> Option<FullName> {
        None
    }
    fn view_write_info(&self, _view: &FullName) -> Option<&crate::sem::ViewWriteInfo> {
        None
    }
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

impl BaseParseContext for BaseContext {
    fn sqlite_relation_identity(&self, _name: &FullName) -> Option<FullName> {
        None
    }
    fn trigger(
        &self,
        _identity: &crate::sem::TriggerIdentity,
    ) -> Option<&crate::sem::CreateTrigger> {
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

impl crate::sem::ConflictInfoRead for BaseContext {
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
        crate::sem::parse_context::conflict_info::infer_from_catalog(self, table, conflict)
    }
}

impl crate::sem::ViewInsertInfoRead for BaseContext {
    fn view_insert_support(&self, _view: &FullName) -> Option<&crate::sem::ViewInsertSupport> {
        None
    }
}

impl crate::sem::ViewUpdateInfoRead for BaseContext {
    fn view_update_support(
        &self,
        _view: &crate::sem::FullName,
    ) -> Option<&crate::sem::ViewUpdateSupport> {
        None
    }
}

impl crate::sem::ViewDeleteInfoRead for BaseContext {
    fn view_delete_support(&self, _view: &crate::sem::FullName) -> crate::sem::ViewDeleteSupport {
        todo!("catalog deletable views, INSTEAD OF DELETE triggers and rules")
    }
}

impl crate::sem::ViewMergeInfoRead for BaseContext {
    fn view_merge_support(&self, _view: &crate::sem::FullName) -> crate::sem::ViewMergeSupport {
        todo!("catalog MERGE-eligible views, rules, triggers and action columns")
    }
}

impl crate::sem::RoleInfoRead for BaseContext {}
impl crate::sem::OwnershipInfoRead for BaseContext {
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

impl crate::sem::ObjectInfoRead for BaseContext {
    fn object_kind(&self, _: &FullName) -> Option<crate::sem::Kind> {
        None
    }
}
impl crate::sem::ExtensionInfoRead for BaseContext {
    fn extension_exists(&self, _: &str) -> bool {
        false
    }
}
