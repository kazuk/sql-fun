use crate::sem::{BaseContext, BaseParseContext, FullName, TypeDefinition, TypeReference};
use std::sync::Arc;

/// An owned, immutable schema context that can be shared across threads.
///
/// Cloning a handle shares the same catalog without copying its contents. Each
/// clone keeps the catalog alive independently of the original handle. Reads
/// borrow from the handle, so returned references cannot outlive that handle.
///
/// Convert an initialized [`BaseContext`] by value to freeze its builtin catalog.
/// A frozen context exposes only [`BaseParseContext`] reads; it cannot be thawed
/// or used to extract or mutably access the underlying catalog.
///
/// Retained args and metadata describe how this catalog was built. They are
/// historical evidence, not initialization inputs for a new analysis context.
///
/// # Examples
///
/// Given a builder over an explicitly selected base, freeze it, read from the
/// completed catalog, and share that catalog with the next layer:
///
/// ```
/// use std::path::Path;
/// use ::sql_fun_sqlast::sem::{BaseParseContext, FrozenBaseContext, ParseContext, SchemaFileContext};
///
/// fn next_layer(builder: SchemaFileContext, args: &::sql_fun_core::SqlFunArgs, metadata: &::sql_fun_core::SqlFunMetadata) {
///     let frozen = builder.freeze();
///     let _metadata = frozen.metadata();
///     let shared = frozen.clone();
///     let _next = SchemaFileContext::new(Path::new("schema.sql"), &shared, args, metadata);
/// }
///
/// fn assert_shareable<T: Clone + std::fmt::Debug + Send + Sync>() {}
/// assert_shareable::<FrozenBaseContext>();
/// ```
///
/// Frozen handles expose no parsing mutators, even when owned uniquely:
///
/// ```compile_fail
/// use ::sql_fun_sqlast::sem::{FrozenBaseContext, ParseContext};
///
/// fn mutate_frozen(mut frozen: FrozenBaseContext) {
///     frozen.set_search_path(&[]);
/// }
/// ```
///
/// Their inner storage is private and cannot be extracted:
///
/// ```compile_fail
/// use ::sql_fun_sqlast::sem::FrozenBaseContext;
///
/// fn extract_storage(frozen: FrozenBaseContext) {
///     let _inner = frozen.0;
/// }
/// ```
/// Metadata and arguments are immutable evidence of the parameters used to
/// analyze the retained schema objects. Consumers must preserve this evidence.
#[derive(Clone, Debug)]
pub struct FrozenBaseContext(Arc<dyn BaseParseContext + Send + Sync>);

impl FrozenBaseContext {
    // Freezing ends analysis and seals its results together with the parameter
    // evidence used to interpret them. This constructor must remain crate-private:
    // exposing it would let external consumers wrap an analysis result with
    // substituted metadata or arguments, breaking that sealed evidence.
    pub(crate) fn new<T>(context: T) -> Self
    where
        T: BaseParseContext + Send + Sync + 'static,
    {
        Self(Arc::new(context))
    }

    fn catalog(&self) -> &(dyn BaseParseContext + Send + Sync) {
        self.0.as_ref()
    }
}

impl From<BaseContext> for FrozenBaseContext {
    fn from(context: BaseContext) -> Self {
        Self::new(context)
    }
}

#[cfg(test)]
mod test_frozen_root {
    use crate::sem::{
        BaseParseContext, FrozenBaseContext, FullName, GetSourcePathWithSpanOpt, SearchPathRead,
    };

    #[test]
    fn clone_retains_selected_metadata_and_builtin_reads() -> testresult::TestResult {
        fn assert_shareable<T: Clone + Send + Sync>() {}
        assert_shareable::<FrozenBaseContext>();

        let frozen = test_helpers::selected_root()?;
        let clone = frozen.clone();
        drop(frozen);

        assert_eq!(clone.metadata().name(), "analytics");
        assert_eq!(
            clone.metadata().connector().as_deref(),
            Some("selected-connector")
        );
        assert_eq!(clone.get_search_path()[0].as_str(), "analytics");
        assert_eq!(
            clone.get_default_schema().expect("default schema").as_str(),
            "analytics"
        );
        assert!(
            clone
                .get_type(&FullName::with_schema("pg_catalog", "int4"))
                .is_some()
        );
        assert!(clone.get_source_file_path().is_none());
        assert!(clone.get_source_path_with_span().is_none());
        Ok(())
    }

    mod test_helpers {
        use crate::sem::{BaseContext, FrozenBaseContext};
        use ::sql_fun_core::{SqlFunArgs, SqlFunMetadataCollection};
        use std::path::Path;

        pub(super) fn selected_root() -> Result<FrozenBaseContext, Box<dyn std::error::Error>> {
            let fixture =
                Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/selected-postgres.toml");
            let mut metadata = SqlFunMetadataCollection::load_from(fixture)?;
            let selected = metadata.remove("analytics").expect("selected metadata");
            let args = <SqlFunArgs as ::clap::Parser>::try_parse_from([
                "sqlfun",
                "--metadata-file",
                "this-metadata-file-does-not-exist.toml",
                "--sql-fun-home",
                env!("SQL_FUN_HOME"),
                "--postgres-version",
                "17",
            ])?;
            Ok(BaseContext::new(&args, &selected)?.into())
        }
    }
}

macro_rules! forward_catalog_reads {
    ($context:ty, $catalog:ident) => {
        impl crate::sem::ObjectInfoRead for $context {
            fn object_kind(&self, name: &crate::sem::FullName) -> Option<crate::sem::Kind> {
                self.$catalog().object_kind(name)
            }
        }
        impl crate::sem::ExtensionInfoRead for $context {
            fn extension_exists(&self, name: &str) -> bool {
                self.$catalog().extension_exists(name)
            }
        }

        impl crate::sem::RoleInfoRead for $context {
            fn role_by_name(&self, name: &str) -> Option<&crate::sem::Role> {
                crate::sem::RoleInfoRead::role_by_name(self.$catalog(), name)
            }
        }
        impl crate::sem::OwnershipInfoRead for $context {
            fn schema_definition(&self, name: &str) -> Option<&crate::sem::CreateSchema> {
                crate::sem::OwnershipInfoRead::schema_definition(self.$catalog(), name)
            }

            fn ownership_definitions(
                &self,
                kind: crate::sem::ObjectKind,
                name: &crate::sem::FullName,
            ) -> Vec<crate::sem::OwnedObjectDefinition> {
                crate::sem::OwnershipInfoRead::ownership_definitions(self.$catalog(), kind, name)
            }
            fn routine_definitions(
                &self,
                name: &crate::sem::FullName,
            ) -> Vec<crate::sem::CreateFunction> {
                crate::sem::OwnershipInfoRead::routine_definitions(self.$catalog(), name)
            }
        }
        impl crate::sem::CastInfoRead for $context {
            fn get_assignment_cast(
                &self,
                source: &crate::sem::TypeReference,
                target: &crate::sem::TypeReference,
            ) -> Option<crate::sem::CastDefinition> {
                crate::sem::CastInfoRead::get_assignment_cast(self.$catalog(), source, target)
            }
            fn cast_sources(
                &self,
                target: &crate::sem::TypeReference,
                explicit: bool,
            ) -> std::collections::HashSet<crate::sem::TypeReference> {
                crate::sem::CastInfoRead::cast_sources(self.$catalog(), target, explicit)
            }

            fn get_explicit_cast(
                &self,
                source: &crate::sem::TypeReference,
                target: &crate::sem::TypeReference,
            ) -> Option<crate::sem::CastDefinition> {
                crate::sem::CastInfoRead::get_explicit_cast(self.$catalog(), source, target)
            }

            fn get_implicit_cast(
                &self,
                source: &crate::sem::TypeReference,
                target: &crate::sem::TypeReference,
            ) -> Option<crate::sem::CastDefinition> {
                crate::sem::CastInfoRead::get_implicit_cast(self.$catalog(), source, target)
            }

            fn get_implicit_castable(
                &self,
                source: &crate::sem::TypeReference,
            ) -> std::collections::HashSet<crate::sem::TypeReference> {
                crate::sem::CastInfoRead::get_implicit_castable(self.$catalog(), source)
            }
        }

        impl crate::sem::FunctionInfoRead for $context {
            fn function_preserves_view_rows(
                &self,
                name: &crate::sem::FullName,
                overload: &crate::sem::OverloadVariant,
            ) -> bool {
                crate::sem::FunctionInfoRead::function_preserves_view_rows(
                    self.$catalog(),
                    name,
                    overload,
                )
            }

            fn get_function_impl(
                &self,
                name: &crate::sem::FullName,
            ) -> Option<crate::sem::FunctionOverloadCollection> {
                crate::sem::FunctionInfoRead::get_function_impl(self.$catalog(), name)
            }
        }

        impl crate::sem::ConflictInfoRead for $context {
            fn database_collation(&self) -> Option<&crate::sem::DatabaseCollation> {
                self.$catalog().database_collation()
            }
            fn collation_definition(
                &self,
                name: &crate::sem::FullName,
            ) -> Option<&crate::sem::CollationDefinition> {
                self.$catalog().collation_definition(name)
            }
            fn operator_class_definition(
                &self,
                name: &crate::sem::FullName,
                method: &str,
            ) -> Option<&crate::sem::OperatorClassDefinition> {
                self.$catalog().operator_class_definition(name, method)
            }
            fn lookup_sqlite_collation(
                &self,
                name: &crate::sem::FullName,
            ) -> crate::sem::CatalogNameLookup {
                self.$catalog().lookup_sqlite_collation(name)
            }
            fn lookup_collation(
                &self,
                name: &crate::sem::FullName,
            ) -> crate::sem::CatalogNameLookup {
                crate::sem::ConflictInfoRead::lookup_collation(self.$catalog(), name)
            }
            fn lookup_operator_class(
                &self,
                name: &crate::sem::FullName,
                method: &str,
            ) -> crate::sem::CatalogNameLookup {
                crate::sem::ConflictInfoRead::lookup_operator_class(self.$catalog(), name, method)
            }
            fn infer_conflict_arbiters(
                &self,
                table: &crate::sem::TableName,
                conflict: &crate::sem::InsertConflict,
            ) -> Result<crate::sem::ConflictArbiterInference, crate::sem::AnalysisError> {
                crate::sem::ConflictInfoRead::infer_conflict_arbiters(
                    self.$catalog(),
                    table,
                    conflict,
                )
            }
        }

        impl crate::sem::IndexInfoRead for $context {
            fn indexes_for_table(
                &self,
                table: &crate::sem::TableName,
            ) -> Vec<&crate::sem::CreateIndex> {
                crate::sem::IndexInfoRead::indexes_for_table(self.$catalog(), table)
            }
            fn get_index_impl(
                &self,
                name: &crate::sem::FullName,
            ) -> Option<&crate::sem::CreateIndex> {
                crate::sem::IndexInfoRead::get_index_impl(self.$catalog(), name)
            }
        }

        impl crate::sem::OperatorInfoRead for $context {
            fn get_operator_op(
                &self,
                name: &crate::sem::OperatorName,
                left: bool,
                right: bool,
            ) -> Option<&crate::sem::OperatorDefinition> {
                crate::sem::OperatorInfoRead::get_operator_op(self.$catalog(), name, left, right)
            }
        }

        impl crate::sem::SequenceInfoRead for $context {
            fn sequences_owned_by(
                &self,
                table: &crate::sem::TableName,
            ) -> Vec<crate::sem::CreateSequence> {
                crate::sem::SequenceInfoRead::sequences_owned_by(self.$catalog(), table)
            }

            fn get_sequence_impl(
                &self,
                name: &crate::sem::FullName,
            ) -> Option<&crate::sem::CreateSequence> {
                crate::sem::SequenceInfoRead::get_sequence_impl(self.$catalog(), name)
            }
        }

        impl crate::sem::TableInfoRead for $context {
            fn get_table_impl(
                &self,
                name: &crate::sem::FullName,
            ) -> Option<&crate::sem::CreateTable> {
                crate::sem::TableInfoRead::get_table_impl(self.$catalog(), name)
            }
        }

        impl crate::sem::ViewInsertInfoRead for $context {
            fn view_insert_support(
                &self,
                name: &crate::sem::FullName,
            ) -> Option<&crate::sem::ViewInsertSupport> {
                crate::sem::ViewInsertInfoRead::view_insert_support(self.$catalog(), name)
            }
        }

        impl crate::sem::ViewDeleteInfoRead for $context {
            fn view_delete_support(
                &self,
                name: &crate::sem::FullName,
            ) -> crate::sem::ViewDeleteSupport {
                crate::sem::ViewDeleteInfoRead::view_delete_support(self.$catalog(), name)
            }
        }
        impl crate::sem::ViewUpdateInfoRead for $context {
            fn view_update_support(
                &self,
                name: &crate::sem::FullName,
            ) -> Option<&crate::sem::ViewUpdateSupport> {
                crate::sem::ViewUpdateInfoRead::view_update_support(self.$catalog(), name)
            }
        }

        impl crate::sem::ViewMergeInfoRead for $context {
            fn view_merge_support(
                &self,
                name: &crate::sem::FullName,
            ) -> crate::sem::ViewMergeSupport {
                crate::sem::ViewMergeInfoRead::view_merge_support(self.$catalog(), name)
            }
        }

        impl crate::sem::ViewInfoRead for $context {
            fn view_trigger_target(
                &self,
                trigger: &crate::sem::FullName,
            ) -> Option<crate::sem::FullName> {
                crate::sem::ViewInfoRead::view_trigger_target(self.$catalog(), trigger)
            }
            fn view_write_info(
                &self,
                view: &crate::sem::FullName,
            ) -> Option<&crate::sem::ViewWriteInfo> {
                crate::sem::ViewInfoRead::view_write_info(self.$catalog(), view)
            }
            fn get_view_impl(&self, name: &crate::sem::FullName) -> Option<&crate::sem::ViewLike> {
                crate::sem::ViewInfoRead::get_view_impl(self.$catalog(), name)
            }
        }

        impl crate::sem::SearchPathRead for $context {
            fn get_search_path(&self) -> &[crate::sem::SchemaName] {
                crate::sem::SearchPathRead::get_search_path(self.$catalog())
            }

            fn get_default_schema(&self) -> Option<crate::sem::SchemaName> {
                crate::sem::SearchPathRead::get_default_schema(self.$catalog())
            }
        }

        impl crate::sem::GetSourcePathWithSpanOpt for $context {
            fn get_source_path_with_span(&self) -> Option<&crate::sem::SourcePathWithSpan> {
                crate::sem::GetSourcePathWithSpanOpt::get_source_path_with_span(self.$catalog())
            }
        }

        impl crate::sem::RuleInfoRead for $context {
            fn local_rule(
                &self,
                identity: &crate::sem::RuleIdentity,
            ) -> Option<Option<&crate::sem::CreateRule>> {
                crate::sem::RuleInfoRead::local_rule(self.$catalog(), identity)
            }
            fn rule_parent(&self) -> Option<&dyn crate::sem::RuleInfoRead> {
                crate::sem::RuleInfoRead::rule_parent(self.$catalog())
            }
        }

        impl crate::sem::BaseParseContext for $context {
            fn sqlite_relation_identity(&self, name: &FullName) -> Option<FullName> {
                self.$catalog().sqlite_relation_identity(name)
            }
            fn trigger(
                &self,
                identity: &crate::sem::TriggerIdentity,
            ) -> Option<&crate::sem::CreateTrigger> {
                self.$catalog().trigger(identity)
            }
            fn metadata(&self) -> &::sql_fun_core::SqlFunMetadata {
                crate::sem::BaseParseContext::metadata(self.$catalog())
            }

            fn get_args(&self) -> &::sql_fun_core::SqlFunArgs {
                crate::sem::BaseParseContext::get_args(self.$catalog())
            }

            fn get_source_file_path(&self) -> Option<&std::path::Path> {
                crate::sem::BaseParseContext::get_source_file_path(self.$catalog())
            }
        }
    };
}

forward_catalog_reads!(FrozenBaseContext, catalog);
forward_catalog_reads!(crate::sem::SchemaFileContext, catalog);
forward_catalog_reads!(crate::ParsedSchemaDump, base_context);

impl crate::sem::TypeInfoRead for FrozenBaseContext {
    fn get_type_impl(&self, name: &FullName) -> Option<&TypeDefinition> {
        self.catalog().get_type_impl(name)
    }

    fn get_array_item_type(&self, name: &TypeReference) -> Option<TypeReference> {
        self.catalog().get_array_item_type(name)
    }
}

#[cfg(test)]
mod test_type_lookup_cost {
    use crate::sem::{BaseParseContext, FullName, ParseContext};
    use std::{path::Path, sync::atomic::Ordering};

    #[rstest::rstest]
    #[case::root(0)]
    #[case::one_layer(1)]
    #[case::eight_layers(8)]
    #[case::thirty_two_layers(32)]
    fn qualified_type_miss_reaches_root_once(#[case] layers: usize) {
        let (mut base, probes) = test_helpers::counted_root();
        for _ in 0..layers {
            base = crate::test_helpers::schema_layer(Path::new("layer.sql"), &base).freeze();
        }

        assert!(
            base.get_type(&FullName::with_schema("absent", "missing_type"))
                .is_none()
        );
        assert_eq!(probes.load(Ordering::Relaxed), 1);
    }

    mod test_helpers {
        use crate::sem::{
            FrozenBaseContext, FullName, ParseContext, TypeDefinition, TypeInfoRead, TypeReference,
        };
        use crate::test_helpers::TestParseContext;
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };

        #[derive(Debug)]
        struct CountingRoot {
            base: FrozenBaseContext,
            probes: Arc<AtomicUsize>,
        }

        impl CountingRoot {
            fn base(&self) -> &FrozenBaseContext {
                &self.base
            }
        }

        forward_catalog_reads!(CountingRoot, base);

        impl TypeInfoRead for CountingRoot {
            fn get_type_impl(&self, name: &FullName) -> Option<&TypeDefinition> {
                assert_eq!(
                    self.probes.fetch_add(1, Ordering::Relaxed),
                    0,
                    "a qualified lookup must not probe the root twice"
                );
                self.base.get_type_impl(name)
            }

            fn get_array_item_type(&self, name: &TypeReference) -> Option<TypeReference> {
                self.base.get_array_item_type(name)
            }
        }

        pub(super) fn counted_root() -> (FrozenBaseContext, Arc<AtomicUsize>) {
            let probes = Arc::new(AtomicUsize::new(0));
            let root = CountingRoot {
                base: TestParseContext::default().freeze(),
                probes: Arc::clone(&probes),
            };
            (FrozenBaseContext::new(root), probes)
        }
    }
}
