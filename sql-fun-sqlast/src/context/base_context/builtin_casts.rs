use crate::context::{
    ConflictMasters,
    base_context::{
        BaseContext as BaseContextBaseContext, CastDefinition as BaseContextCastDefinition,
        deserialize_from_path,
    },
};
use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

use crate::{
    TrieMap,
    context::{CastInfoRead, TypeInfoRead},
    names::FullName,
    sem::{AnalysisError, CastContext, CastDefinition, TypeReference},
};

/// Loads builtin cast definitions from the selected master file.
pub struct PgBuiltinCasts {}

#[expect(dead_code)]
#[derive(Debug, serde::Deserialize)]
pub struct CastInfo {
    pub func: String,
    pub method: String,
    pub context: String,
    #[serde(default, rename = "is_prefferd")]
    preferred: bool,
}

// alias: source_type -> target_type -> CastInfo
pub type CastMap = HashMap<String, HashMap<String, CastInfo>>;

type LoadedCasts = (TrieMap<TrieMap<CastDefinition>>, HashSet<FullName>);

impl PgBuiltinCasts {
    pub(super) fn load<P: AsRef<Path>>(
        path: P,
        masters: &ConflictMasters,
    ) -> Result<LoadedCasts, AnalysisError> {
        let casts: CastMap = deserialize_from_path(path)?;

        let mut result = TrieMap::new();
        let mut binary_ordered = HashSet::new();
        for (source, m) in casts {
            let mut group = TrieMap::new();
            for (target, ci) in m {
                if ci.method == "b"
                    && ci.context == "i"
                    && masters.type_ordered(&FullName::with_schema("pg_catalog", &target))
                {
                    binary_ordered.insert(FullName::with_schema("pg_catalog", &source));
                }
                let cc = match ci.context.as_str() {
                    "i" if ci.preferred => CastContext::ImplicitPrefeerd,
                    "i" => CastContext::Implicit,
                    "a" => CastContext::Assignment,
                    "e" => CastContext::Explicit,
                    e => panic!("unexpected cast context {e}"),
                };
                let def = CastDefinition::new(cc);
                group.insert(&target, def);
            }
            result.insert(&source, group);
        }
        Ok((result, binary_ordered))
    }
}

#[cfg(test)]
mod test_load {
    use crate::{
        context::{ConflictMasters, base_context::builtin_casts::PgBuiltinCasts},
        sem::CastContext,
    };

    use crate::test_helpers::cast_catalog as test_helpers;

    #[rstest::rstest]
    #[case::preferred_implicit("i", Some(true), CastContext::ImplicitPrefeerd)]
    #[case::ordinary_implicit("i", Some(false), CastContext::Implicit)]
    #[case::legacy_implicit("i", None, CastContext::Implicit)]
    #[case::preferred_assignment("a", Some(true), CastContext::Assignment)]
    #[case::preferred_explicit("e", Some(true), CastContext::Explicit)]
    fn loader_preserves_preference_only_for_implicit_casts(
        #[case] context: &str,
        #[case] preferred: Option<bool>,
        #[case] expected: CastContext,
    ) -> testresult::TestResult {
        let catalog = test_helpers::cast_catalog(context, preferred)?;
        let (casts, _) = PgBuiltinCasts::load(catalog.path(), &ConflictMasters::sqlite())?;
        let cast = casts.get("source").unwrap().get("target").unwrap();
        assert_eq!(cast.context(), expected);
        assert_eq!(cast.use_in_implicit(), expected.use_in_implicit());
        assert_eq!(
            cast.context().overload_priority_score(),
            expected.overload_priority_score()
        );
        Ok(())
    }
}

impl CastInfoRead for BaseContextBaseContext {
    fn get_assignment_cast(
        &self,
        source: &TypeReference,
        target: &TypeReference,
    ) -> Option<CastDefinition> {
        let cast = self.get_explicit_cast(source, target)?;
        if cast.context().use_in_assignment() {
            return Some(cast);
        }
        let (name, array) = self.builtin_cast_type(target)?;
        if !array && matches!(name, "text" | "varchar" | "bpchar" | "name") {
            return Some(CastDefinition::new(CastContext::Assignment));
        }
        None
    }

    fn declared_implicit_cast_sources(&self, target: &TypeReference) -> HashSet<TypeReference> {
        // The implicit branch reads declared builtin casts, never the type catalog.
        self.cast_sources(target, false)
    }

    fn cast_sources(&self, target: &TypeReference, explicit: bool) -> HashSet<TypeReference> {
        let mut sources = HashSet::from([target.clone()]);
        for (name, _) in self.casts.iter() {
            let source = TypeReference::concrete_type_ref(
                &FullName::with_schema("pg_catalog", &name),
                target.is_array() == Some(true),
            );
            let cast = if explicit {
                self.get_explicit_cast(&source, target)
            } else {
                self.get_implicit_cast(&source, target)
            };
            if cast.is_some() {
                sources.insert(source);
            }
        }
        for vector in ["int2vector", "oidvector"] {
            let source = TypeReference::from_master_type_name(vector);
            let cast = if explicit {
                self.get_explicit_cast(&source, target)
            } else {
                self.get_implicit_cast(&source, target)
            };
            if cast.is_some() {
                sources.insert(source);
            }
        }
        if explicit {
            for (_, definition) in self.types.iter() {
                if let Some(source) = definition.type_reference()
                    && self.get_explicit_cast(source, target).is_some()
                {
                    sources.insert(source.clone());
                }
            }
        }
        sources
    }

    fn get_explicit_cast(
        &self,
        source_type: &TypeReference,
        target_type: &TypeReference,
    ) -> Option<BaseContextCastDefinition> {
        if !source_type.is_dynamic() && source_type == target_type {
            return Some(CastDefinition::new(CastContext::NoConversion));
        }
        let (source_local_name, source_array) = self.builtin_cast_type(source_type)?;
        let (target_local_name, target_array) = self.builtin_cast_type(target_type)?;
        if source_local_name == target_local_name && source_array == target_array {
            return Some(CastDefinition::new(CastContext::NoConversion));
        }
        if source_array == target_array
            && let Some(targets) = self.casts.get(source_local_name)
            && let Some(cast) = targets.get(target_local_name)
            && cast.use_in_explicit()
        {
            return Some(cast.clone());
        }
        if let Some(cast) = self.vector_array_cast(source_type, target_type, false) {
            return Some(cast);
        }
        // PostgreSQL find_coercion_pathway permits explicit I/O coercion
        // to or from the builtin string category even without a pg_cast row.
        let source_string =
            !source_array && matches!(source_local_name, "text" | "varchar" | "bpchar" | "name");
        let target_string =
            !target_array && matches!(target_local_name, "text" | "varchar" | "bpchar" | "name");
        if source_string || target_string {
            Some(CastDefinition::new(CastContext::Explicit))
        } else {
            None
        }
    }

    fn get_implicit_cast(
        &self,
        source_type: &TypeReference,
        target_type: &TypeReference,
    ) -> Option<BaseContextCastDefinition> {
        if !source_type.is_dynamic() && source_type == target_type {
            return Some(CastDefinition::new(CastContext::NoConversion));
        }
        let (source_local_name, source_array) = self.builtin_cast_type(source_type)?;
        let (target_local_name, target_array) = self.builtin_cast_type(target_type)?;
        if source_local_name == target_local_name && source_array == target_array {
            return Some(CastDefinition::new(CastContext::NoConversion));
        }
        if source_array != target_array {
            return self.vector_array_cast(source_type, target_type, true);
        }

        let from_source = self.casts.get(source_local_name)?;
        let cast_def = from_source.get(target_local_name)?;

        if cast_def.use_in_implicit() {
            Some(cast_def.clone())
        } else {
            None
        }
    }

    fn get_implicit_castable(&self, source_type: &TypeReference) -> HashSet<TypeReference> {
        let mut results = HashSet::new();
        let Some((source_local_name, source_array)) = self.builtin_cast_type(source_type) else {
            return results;
        };
        results.insert(TypeReference::concrete_type_ref(
            &FullName::with_schema("pg_catalog", source_local_name),
            source_array,
        ));
        if !source_array && let Some(element) = self.get_array_item_type(source_type) {
            results.extend(
                self.get_implicit_castable(&element)
                    .into_iter()
                    .map(|ty| ty.array_type()),
            );
        }
        if let Some(from_source) = self.casts.get(source_local_name) {
            for (target, cast) in from_source.iter() {
                if cast.use_in_implicit() {
                    results.insert(TypeReference::concrete_type_ref(
                        &FullName::with_schema("pg_catalog", &target),
                        source_array,
                    ));
                }
            }
        }
        results
    }
}

#[cfg(test)]
mod test_builtin_cast_identity {
    use crate::{
        context::{BaseContext, CastInfoRead, FrozenBaseContext, SchemaFileContext},
        names::FullName,
        sem::TypeReference,
    };
    use ::clap::Parser;
    use ::sql_fun_core::SqlFunArgs;
    use std::path::{Path, PathBuf};

    #[test]
    fn keeps_array_and_schema_identity_and_cast_context() {
        let metadata = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("sql_fun.metadata.toml");
        let args = SqlFunArgs::try_parse_from([
            "sqlfun",
            "--metadata-file",
            metadata.to_str().expect("metadata path is UTF-8"),
            "--sql-fun-home",
            env!("SQL_FUN_HOME"),
        ])
        .expect("test arguments");
        let collection = args.load_metadata().expect("fixture metadata");
        let selected = collection.get("default").expect("selected test metadata");
        let context = BaseContext::new(&args, selected).expect("builtin context");
        let text = TypeReference::from_master_type_name("text");
        let text_array = TypeReference::from_master_type_name("_text");
        let public_text = TypeReference::from_full_name(FullName::with_schema("public", "text"));
        let oid = TypeReference::from_master_type_name("oid");
        let regclass = TypeReference::from_master_type_name("regclass");
        let xml = TypeReference::from_master_type_name("xml");
        let mood = TypeReference::from_full_name(FullName::with_schema("public", "mood"));
        let smallint_array = TypeReference::from_master_type_name("_int2");
        let integer_array = TypeReference::from_master_type_name("_int4");

        assert!(context.get_implicit_cast(&text, &text).is_some());
        assert!(
            context
                .get_implicit_cast(&mood, &mood)
                .is_some_and(|cast| cast.is_no_conversion())
        );
        assert!(
            context
                .get_explicit_cast(&mood, &mood)
                .is_some_and(|cast| cast.is_no_conversion())
        );
        assert!(context.get_implicit_cast(&text, &text_array).is_none());
        assert!(context.get_explicit_cast(&text_array, &text).is_some());
        assert!(context.get_implicit_cast(&public_text, &text).is_none());
        assert!(
            context
                .get_implicit_cast(&smallint_array, &integer_array)
                .is_some()
        );
        assert!(
            context
                .get_explicit_cast(&integer_array, &smallint_array)
                .is_some()
        );
        assert!(
            context
                .get_implicit_cast(&integer_array, &smallint_array)
                .is_none()
        );
        assert!(
            context
                .get_implicit_castable(&smallint_array)
                .contains(&integer_array)
        );
        assert!(
            !context
                .get_implicit_castable(&smallint_array)
                .contains(&TypeReference::from_master_type_name("int4"))
        );
        assert!(context.get_implicit_cast(&oid, &regclass).is_some());
        assert!(context.get_implicit_castable(&text).contains(&regclass));
        assert!(!context.get_implicit_castable(&text).contains(&xml));

        let schema_context = {
            let analysis_base = &FrozenBaseContext::from(context);
            SchemaFileContext::new(Path::new("fixture.sql"), analysis_base, &args, selected)
        };
        assert!(schema_context.get_implicit_cast(&mood, &mood).is_some());
        assert!(schema_context.get_explicit_cast(&mood, &mood).is_some());
        assert!(
            schema_context
                .get_implicit_cast(&smallint_array, &integer_array)
                .is_some()
        );
        assert!(
            schema_context
                .get_implicit_castable(&smallint_array)
                .contains(&integer_array)
        );
        assert!(schema_context.get_explicit_cast(&text, &xml).is_some());
        assert!(schema_context.get_implicit_cast(&text, &xml).is_none());
    }
}

impl super::BaseContext {
    // PostgreSQL derives vector-source array conversions from the element cast;
    // the inverse conversion to a vector is deliberately not synthesized.
    fn vector_array_cast(
        &self,
        source: &TypeReference,
        target: &TypeReference,
        implicit: bool,
    ) -> Option<CastDefinition> {
        if source.is_array() != Some(false) || target.is_array() != Some(true) {
            return None;
        }
        let element = self.get_array_item_type(source)?;
        let target_element = self.get_array_item_type(target)?;
        let cast = if implicit {
            self.get_implicit_cast(&element, &target_element)?
        } else {
            self.get_explicit_cast(&element, &target_element)?
        };
        // Equal elements still require a conversion between distinct containers.
        // An element's preferred-conversion score is not a container preference.
        let context = if cast.use_in_implicit() {
            CastContext::implicit()
        } else {
            cast.context()
        };
        Some(CastDefinition::new(context))
    }

    pub(super) fn builtin_cast_type<'a>(&self, ty: &'a TypeReference) -> Option<(&'a str, bool)> {
        if ty.is_dynamic() {
            return None;
        }
        let name = ty.full_name()?;
        if name.catalog().is_some()
            || name
                .schema()
                .is_some_and(|schema| schema.as_str() != "pg_catalog")
        {
            return None;
        }
        let builtin_name = FullName::with_schema("pg_catalog", name.name().as_str());
        self.types.get(&builtin_name.to_string())?;
        Some((name.name().as_str(), ty.is_array()?))
    }
}
