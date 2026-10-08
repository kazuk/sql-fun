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
#[derive(Clone, Debug)]
pub struct FrozenBaseContext(Arc<dyn BaseParseContext + Send + Sync>);

impl FrozenBaseContext {
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
        impl crate::sem::CastInfoRead for $context {
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
            fn get_function_impl(
                &self,
                name: &crate::sem::FullName,
            ) -> Option<crate::sem::FunctionOverloadCollection> {
                crate::sem::FunctionInfoRead::get_function_impl(self.$catalog(), name)
            }
        }

        impl crate::sem::IndexInfoRead for $context {
            fn get_index(&self, name: &str) -> Option<&crate::sem::CreateIndex> {
                crate::sem::IndexInfoRead::get_index(self.$catalog(), name)
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

        impl crate::sem::ViewInfoRead for $context {
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

        impl crate::sem::BaseParseContext for $context {
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
    use crate::sem::{BaseParseContext, FullName, ParseContext, SchemaFileContext};
    use std::{path::Path, sync::atomic::Ordering};

    #[rstest::rstest]
    #[case::root(0)]
    #[case::one_layer(1)]
    #[case::eight_layers(8)]
    #[case::thirty_two_layers(32)]
    fn qualified_type_miss_reaches_root_once(#[case] layers: usize) {
        let (mut base, probes) = test_helpers::counted_root();
        for _ in 0..layers {
            base = SchemaFileContext::new(Path::new("layer.sql"), &base).freeze();
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
