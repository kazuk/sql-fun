use super::ConversionError;
use crate::sem::{
    CastContext, CastDefinition, FullName, SchemaName, TypeDefinition as LegacyDefinition,
};
use crate::types::{
    Any, Candidates, EnumType, Never, TypeContext, array::ArrayType, base::BaseType,
};

mod test_helpers {
    use crate::sem::{CreateEnum, FullName, SchemaName, TypeDefinition, TypeReference};
    use crate::test_helpers::TestParseContext;

    pub(super) fn context() -> TestParseContext {
        let mut context = TestParseContext::default().with_dialect(crate::ast::Dialect::postgres());
        context.set_get_search_path_result(&[SchemaName::from("public")]);
        context
    }

    pub(super) fn reference(local: &str) -> TypeReference {
        TypeReference::concrete_type_ref(&FullName::with_schema("public", local), false)
    }

    pub(super) fn base(context: &mut TestParseContext, local: &str) -> TypeReference {
        let reference = reference(local);
        context.set_get_type_impl_result(
            reference.full_name(),
            &TypeDefinition::builtin(reference.full_name()),
        );
        reference
    }

    pub(super) fn enumeration(
        context: &mut TestParseContext,
        local: &str,
        labels: &[&str],
    ) -> TypeReference {
        let reference = reference(local);
        let mut owned_labels = Vec::new();
        for label in labels {
            owned_labels.push((*label).to_owned());
        }
        context.set_get_type_impl_result(
            reference.full_name(),
            &TypeDefinition::from(&CreateEnum::new(reference.full_name(), &owned_labels)),
        );
        reference
    }
    pub(super) fn table_row(context: &mut TestParseContext, registered: bool) -> TypeReference {
        use crate::sem::{ColumnDefinition, ColumnName, CreateTable, CreateTableMode, TableName};
        use crate::test_helpers::source_span;
        let member_type = base(context, "integer");
        let name = reference("row");
        let table_name = TableName::from(name.full_name().clone());
        let table = CreateTable::new(
            &table_name,
            vec![ColumnDefinition::new(
                &Some(ColumnName::new("value")),
                Some(&member_type),
                Some(true),
                &[],
            )],
            CreateTableMode::from_if_not_exists(false),
            false,
            &source_span(),
        );
        context.set_get_table_impl_result(name.full_name(), &Some(table));
        let definition = TypeDefinition::table_ref(&table_name);
        if registered {
            context.set_get_type_impl_result(name.full_name(), &definition);
            name
        } else {
            TypeReference::dynamic(&definition)
        }
    }
    pub(super) fn category(context: &mut TestParseContext, category: &str) -> TypeReference {
        use crate::sem::{
            ColumnCollection, ColumnDefinition, CompositeFieldCollection, CreateCompositeType,
            CreateDomain, TypeReference as LegacyReference,
        };
        let base = base(context, "base");
        let reference = reference("value");
        match category {
            "base" => base.clone(),
            "array" => LegacyReference::concrete_type_ref(base.full_name(), true),
            "enum" => enumeration(context, "value", &["first", "second"]),
            "domain" => {
                context.set_get_type_impl_result(
                    reference.full_name(),
                    &TypeDefinition::from(&CreateDomain::new(
                        &reference,
                        &Some(base.full_name().clone()),
                    )),
                );
                reference
            }
            "composite" => {
                context.set_get_type_impl_result(
                    reference.full_name(),
                    &TypeDefinition::from(&CreateCompositeType::new(
                        reference.full_name(),
                        &CompositeFieldCollection::default(),
                    )),
                );
                reference
            }
            "anonymous" => LegacyReference::dynamic(&TypeDefinition::from(
                &CreateCompositeType::from_fields(&CompositeFieldCollection::default()),
            )),
            "implicit" => {
                LegacyReference::dynamic(&TypeDefinition::from(&ColumnCollection::new(vec![
                    ColumnDefinition::new(&None, Some(&base), Some(true), &[]),
                ])))
            }
            _ => unreachable!(),
        }
    }
    pub(super) fn enum_cast_catalog(dialect: crate::ast::Dialect) -> crate::sem::SchemaFileContext {
        use crate::sem::{
            BaseParseContext, CastContext, CastDefinition, ParseContext, SchemaFileContext,
        };
        use std::path::Path;
        let mut root = TestParseContext::default().with_dialect(dialect);
        let source = enumeration(&mut root, "source", &["first"]);
        let target = enumeration(&mut root, "target", &["second"]);
        root.set_get_implicit_cast_result(
            &source,
            &target,
            Some(CastDefinition::new(CastContext::implicit())),
        );
        root.set_get_implicit_castable_result(
            &source,
            &std::collections::HashSet::from([source.clone(), target]),
        );
        let environment = TestParseContext::default().with_dialect(dialect);
        let root = root.freeze();
        SchemaFileContext::new(
            Path::new("array-casts.sql"),
            &root,
            environment.get_args(),
            environment.metadata(),
        )
    }

    pub(super) mod heavy {
        use crate::sem::{BaseContext, FrozenBaseContext, SchemaFileContext};
        use ::clap::Parser;
        use ::sql_fun_core::SqlFunArgs;
        use std::path::{Path, PathBuf};

        /// Loads real builtin metadata for each caller to verify catalog array casts.
        pub(in crate::types::compat) fn builtin_context() -> SchemaFileContext {
            let metadata = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("sql_fun.metadata.toml");
            let args = SqlFunArgs::try_parse_from([
                "sqlfun",
                "--metadata-file",
                metadata.to_str().expect("metadata path"),
                "--sql-fun-home",
                env!("SQL_FUN_HOME"),
            ])
            .expect("fixture args");
            let collection = args.load_metadata().expect("fixture metadata");
            let selected = collection.get("default").expect("selected metadata");
            let base = FrozenBaseContext::from(
                BaseContext::new(&args, selected).expect("builtin context"),
            );
            SchemaFileContext::new(Path::new("array-casts.sql"), &base, &args, selected)
        }
    }
}

#[rstest::rstest]
#[case::implicit(CastContext::implicit())]
#[case::preferred(CastContext::implicit_preferred())]
#[case::assignment(CastContext::assignment())]
#[case::explicit(CastContext::explicit())]
#[case::identity(CastContext::no_conversion())]
fn public_cast_lookup_accepts_loaded_values_and_preserves_context(
    #[case] cast_context: CastContext,
) {
    let mut catalog = test_helpers::context();
    let source = test_helpers::base(&mut catalog, "source");
    let target = if cast_context.is_no_conversion() {
        source.clone()
    } else {
        test_helpers::base(&mut catalog, "target")
    };
    catalog.set_get_implicit_cast_result(&source, &target, Some(CastDefinition::new(cast_context)));
    let context = TypeContext::new(&catalog);
    let loaded_source = context.type_by_name(source.full_name()).unwrap().unwrap();
    let loaded_target = context.type_by_name(target.full_name()).unwrap().unwrap();
    let cast = context
        .explicit_cast(loaded_source.as_ref(), loaded_target.as_ref())
        .unwrap()
        .into_resolution()
        .unwrap()
        .unwrap();
    assert_eq!(
        cast.context(),
        crate::types::CastContext::from(cast_context)
    );
    assert!(cast.source().same_type(loaded_source.as_ref()));
    assert!(cast.target().same_type(loaded_target.as_ref()));
    assert_eq!(
        context
            .assignment_cast(loaded_source.as_ref(), loaded_target.as_ref())
            .unwrap()
            .into_resolution()
            .unwrap()
            .is_some(),
        cast_context.use_in_assignment()
    );
}

#[test]
fn type_lookup_distinguishes_exact_names_search_path_and_absence() {
    let mut catalog = test_helpers::context();
    let public = test_helpers::base(&mut catalog, "value");
    let other = FullName::with_schema("other", "value");
    catalog.set_get_type_impl_result(&other, &LegacyDefinition::builtin(&other));
    catalog.set_get_search_path_result(&[SchemaName::from("other"), SchemaName::from("public")]);
    let context = TypeContext::new(&catalog);
    let local = FullName::from_local_name("value");
    assert!(context.type_by_exact_name(&local).unwrap().is_none());
    let resolved = context.type_by_name(&local).unwrap().unwrap();
    assert!(resolved.same_type(&BaseType::new(&other)));
    let explicit = context
        .type_by_exact_name(public.full_name())
        .unwrap()
        .unwrap();
    assert!(explicit.same_type(&BaseType::new(public.full_name())));
    assert!(!resolved.same_type(explicit.as_ref()));
    assert!(
        context
            .type_by_name(&FullName::from_local_name("missing"))
            .unwrap()
            .is_none()
    );
}

#[test]
fn present_incomplete_definition_is_an_error() {
    use crate::sem::CreateDomain;
    let mut catalog = test_helpers::context();
    let reference = test_helpers::reference("incomplete");
    catalog.set_get_type_impl_result(
        reference.full_name(),
        &LegacyDefinition::from(&CreateDomain::new(&reference, &None)),
    );
    let context = TypeContext::new(&catalog);
    assert_eq!(
        context.type_by_name(reference.full_name()).unwrap_err(),
        ConversionError::missing_domain_base(reference.full_name())
    );
    assert_eq!(
        context
            .type_by_exact_name(reference.full_name())
            .unwrap_err(),
        ConversionError::missing_domain_base(reference.full_name())
    );
}

#[test]
fn array_item_access_preserves_loaded_structural_values() {
    let catalog = test_helpers::context();
    let context = TypeContext::new(&catalog);
    let any = Any::new();
    let array = ArrayType::new(&any);
    let item = context.array_item_type(&array).unwrap();
    assert!(item.same_type(&any));
    assert!(context.array_item_type(&any).is_none());
    let nested = ArrayType::new(&array);
    assert!(context.array_item_type(&nested).unwrap().same_type(&array));
}

#[test]
fn stale_named_snapshot_is_not_reinterpreted_using_current_catalog() {
    let mut catalog = test_helpers::context();
    let reference = test_helpers::enumeration(&mut catalog, "state", &["current"]);
    let stale = EnumType::new(reference.full_name(), &["old".to_owned()]);
    let context = TypeContext::new(&catalog);
    let current = context
        .type_by_name(reference.full_name())
        .unwrap()
        .unwrap();
    assert_eq!(
        context.explicit_cast(&stale, current.as_ref()).unwrap_err(),
        crate::types::CastQueryError::provider(ConversionError::loaded_type_mismatch())
    );
    assert_eq!(
        context.implicit_castable(&stale).unwrap_err(),
        crate::types::CastQueryError::provider(ConversionError::loaded_type_mismatch())
    );
}

#[rstest::rstest]
#[case::any(Box::new(Any::new()) as crate::types::TypeReference)]
#[case::never(Box::new(Never::new()) as crate::types::TypeReference)]
#[case::candidates(Candidates::from_candidates(&[Box::new(Any::new())]))]
fn constraint_values_cannot_become_catalog_cast_endpoints(
    #[case] value: crate::types::TypeReference,
) {
    let catalog = test_helpers::context();
    let context = TypeContext::new(&catalog);
    assert!(
        context
            .explicit_cast(value.as_ref(), value.as_ref())
            .unwrap()
            .into_resolution()
            .unwrap()
            .is_none()
    );
    assert!(
        context
            .implicit_castable(value.as_ref())
            .unwrap()
            .into_resolution()
            .unwrap()
            .as_any()
            .is::<Never>()
    );
}

#[test]
fn implicit_castable_loads_candidate_outputs_without_selecting_one() {
    use std::collections::HashSet;
    let mut catalog = test_helpers::context();
    let source = test_helpers::base(&mut catalog, "source");
    let target = test_helpers::enumeration(&mut catalog, "target", &["first", "second"]);
    catalog.set_get_implicit_castable_result(
        &source,
        &HashSet::from([source.clone(), target.clone()]),
    );
    catalog.set_get_implicit_cast_result(
        &source,
        &target,
        Some(CastDefinition::new(CastContext::implicit())),
    );
    let context = TypeContext::new(&catalog);
    let loaded_source = context.type_by_name(source.full_name()).unwrap().unwrap();
    let loaded_target = context.type_by_name(target.full_name()).unwrap().unwrap();
    let expected = Candidates::from_candidates(&[loaded_source.clone(), loaded_target]);
    let candidates = context
        .implicit_castable(loaded_source.as_ref())
        .unwrap()
        .into_resolution()
        .unwrap();
    assert!(candidates.same_type(expected.as_ref()));
    assert!(!candidates.same_type(loaded_source.as_ref()));
}

#[test]
fn empty_provider_implicit_targets_retain_intrinsic_identity() {
    let mut catalog = test_helpers::context();
    let source = test_helpers::base(&mut catalog, "source");
    let context = TypeContext::new(&catalog);
    let loaded = context.type_by_name(source.full_name()).unwrap().unwrap();
    let expected = Candidates::from_candidates(std::slice::from_ref(&loaded));
    assert!(
        context
            .implicit_castable(loaded.as_ref())
            .unwrap()
            .into_resolution()
            .unwrap()
            .same_type(expected.as_ref())
    );
}

#[test]
fn missing_implicit_candidate_returns_error_instead_of_partial_values() {
    use std::collections::HashSet;
    let mut catalog = test_helpers::context();
    let source = test_helpers::base(&mut catalog, "source");
    let missing = test_helpers::reference("missing");
    catalog.set_get_implicit_castable_result(
        &source,
        &HashSet::from([source.clone(), missing.clone()]),
    );
    let context = TypeContext::new(&catalog);
    let loaded = context.type_by_name(source.full_name()).unwrap().unwrap();
    assert_eq!(
        context.implicit_castable(loaded.as_ref()).unwrap_err(),
        crate::types::CastQueryError::provider(ConversionError::type_not_found(
            missing.full_name()
        ))
    );
}

#[rstest::rstest]
fn source_candidates_are_separate_from_cast_result(#[values(false, true)] explicit: bool) {
    let mut catalog = test_helpers::context();
    let source = test_helpers::enumeration(&mut catalog, "source", &["first"]);
    let target = test_helpers::base(&mut catalog, "target");
    catalog.set_get_implicit_cast_result(
        &source,
        &target,
        Some(CastDefinition::new(CastContext::implicit())),
    );
    let context = TypeContext::new(&catalog);
    let loaded_source = context.type_by_name(source.full_name()).unwrap().unwrap();
    let loaded_target = context.type_by_name(target.full_name()).unwrap().unwrap();
    let expected = Candidates::from_candidates(&[loaded_source, loaded_target.clone()]);
    assert!(
        context
            .cast_sources(loaded_target.as_ref(), explicit)
            .unwrap()
            .into_resolution()
            .unwrap()
            .same_type(expected.as_ref())
    );
    assert!(
        context
            .assignment_sources(loaded_target.as_ref())
            .unwrap()
            .into_resolution()
            .unwrap()
            .same_type(expected.as_ref())
    );
    assert!(!expected.same_type(loaded_target.as_ref()));
}

#[rstest::rstest]
#[case::base("base")]
#[case::array("array")]
#[case::enumeration("enum")]
#[case::domain("domain")]
#[case::named_composite("composite")]
#[case::anonymous_composite("anonymous")]
#[case::implicit_composite("implicit")]
fn loaded_category_endpoints_survive_public_cast_lookup(#[case] category: &str) {
    let mut catalog = test_helpers::context();
    let reference = test_helpers::category(&mut catalog, category);
    catalog.set_get_implicit_cast_result(
        &reference,
        &reference,
        Some(CastDefinition::new(CastContext::no_conversion())),
    );
    let value = super::load_reference(&catalog, &reference).unwrap();
    let context = TypeContext::new(&catalog);
    let cast = context
        .implicit_cast(value.as_ref(), value.as_ref())
        .unwrap()
        .into_resolution()
        .unwrap()
        .unwrap();
    assert!(cast.source().same_type(value.as_ref()));
    assert!(cast.target().same_type(value.as_ref()));
    assert!(cast.context().is_no_conversion());
}

#[rstest::rstest]
fn table_row_endpoints_keep_members_with_or_without_type_registration(
    #[values(false, true)] registered: bool,
) {
    use crate::types::TableRowType;
    let mut catalog = test_helpers::context();
    let reference = test_helpers::table_row(&mut catalog, registered);
    catalog.set_get_implicit_cast_result(
        &reference,
        &reference,
        Some(CastDefinition::new(CastContext::no_conversion())),
    );
    let loaded = super::load_reference(&catalog, &reference).unwrap();
    let context = TypeContext::new(&catalog);
    let cast = context
        .implicit_cast(loaded.as_ref(), loaded.as_ref())
        .unwrap()
        .into_resolution()
        .unwrap()
        .unwrap();
    assert!(cast.source().same_type(loaded.as_ref()));
    assert!(cast.target().same_type(loaded.as_ref()));
    let row = cast
        .source()
        .as_any()
        .downcast_ref::<TableRowType>()
        .unwrap();
    let member = row.members().member("value").unwrap();
    assert_eq!(member.is_not_null(), Some(true));
    assert!(member.member_type().as_any().is::<BaseType>());
}

#[test]
fn nested_array_cast_uses_loaded_components_without_legacy_projection() {
    let mut catalog = test_helpers::context();
    let reference = test_helpers::base(&mut catalog, "integer");
    catalog.set_get_implicit_cast_result(
        &reference,
        &reference,
        Some(CastDefinition::new(CastContext::no_conversion())),
    );
    let base = BaseType::new(reference.full_name());
    let array = ArrayType::new(&base);
    let nested = ArrayType::new(&array);
    let context = TypeContext::new(&catalog);
    let cast = context
        .explicit_cast(&nested, &nested)
        .unwrap()
        .into_resolution()
        .unwrap()
        .unwrap();
    assert!(cast.context().is_no_conversion());
    assert!(cast.source().same_type(&nested));
    assert!(cast.target().same_type(&nested));
}

#[test]
fn changed_nested_member_snapshot_is_rejected() {
    use crate::sem::{
        ColumnName, CompositeFieldCollection, CompositeFieldDefinition, CreateCompositeType,
    };
    use crate::types::{CompositeType, Member, Members};
    let mut catalog = test_helpers::context();
    let base = test_helpers::base(&mut catalog, "integer");
    let name = test_helpers::reference("record");
    let mut fields = CompositeFieldCollection::default();
    fields.push(CompositeFieldDefinition::new(
        "value",
        &Some(base.clone()),
        &Some(true),
    ));
    catalog.set_get_type_impl_result(
        name.full_name(),
        &LegacyDefinition::from(&CreateCompositeType::new(name.full_name(), &fields)),
    );
    let stale_members = Members::new(&[Member::new(
        &Some(ColumnName::new("value")),
        &BaseType::new(base.full_name()),
        Some(false),
    )]);
    let stale = CompositeType::new(&Some(name.full_name().clone()), &stale_members);
    let context = TypeContext::new(&catalog);
    let current = context.type_by_name(name.full_name()).unwrap().unwrap();
    assert_eq!(
        context.explicit_cast(&stale, current.as_ref()).unwrap_err(),
        crate::types::CastQueryError::provider(ConversionError::loaded_type_mismatch())
    );
}

#[test]
fn array_of_unregistered_table_row_preserves_members_in_public_cast() {
    use crate::sem::{TableName, TypeReference as LegacyReference};
    use crate::types::TableRowType;
    let mut catalog = test_helpers::context();
    test_helpers::table_row(&mut catalog, false);
    let name = test_helpers::reference("row");
    let table_name = TableName::from(name.full_name().clone());
    let reference =
        LegacyReference::dynamic(&LegacyDefinition::table_ref_with_array(&table_name, true));
    catalog.set_get_implicit_cast_result(
        &reference,
        &reference,
        Some(CastDefinition::new(CastContext::no_conversion())),
    );
    let loaded = super::load_reference(&catalog, &reference).unwrap();
    let context = TypeContext::new(&catalog);
    let cast = context
        .implicit_cast(loaded.as_ref(), loaded.as_ref())
        .unwrap()
        .into_resolution()
        .unwrap()
        .unwrap();
    assert!(cast.source().same_type(loaded.as_ref()));
    assert!(cast.target().same_type(loaded.as_ref()));
    let array = cast.source().as_any().downcast_ref::<ArrayType>().unwrap();
    let row = array
        .element_type()
        .as_any()
        .downcast_ref::<TableRowType>()
        .unwrap();
    let member = row.members().member("value").unwrap();
    assert_eq!(member.is_not_null(), Some(true));
    assert!(member.member_type().as_any().is::<BaseType>());
}

#[rstest::rstest]
#[case::implicit(
    "int2",
    "int4",
    crate::types::CastContext::implicit(),
    Some(crate::types::CastContext::implicit()),
    true
)]
#[case::assignment(
    "int4",
    "int2",
    crate::types::CastContext::assignment(),
    Some(crate::types::CastContext::assignment()),
    false
)]
#[case::assignment_io(
    "int4",
    "text",
    crate::types::CastContext::explicit(),
    Some(crate::types::CastContext::assignment()),
    false
)]
#[case::explicit_io("text", "int4", crate::types::CastContext::explicit(), None, false)]
fn builtin_array_casts_preserve_element_cast_context(
    #[case] source: &str,
    #[case] target: &str,
    #[case] expected: crate::types::CastContext,
    #[case] assignment: Option<crate::types::CastContext>,
    #[case] implicit: bool,
) {
    // Real builtin metadata supplies the element cast and I/O eligibility.
    let catalog = test_helpers::heavy::builtin_context();
    let context = TypeContext::new(&catalog);
    let source = context
        .type_by_name(&FullName::with_schema("pg_catalog", source))
        .unwrap()
        .unwrap();
    let target = context
        .type_by_name(&FullName::with_schema("pg_catalog", target))
        .unwrap()
        .unwrap();
    let source_array = ArrayType::new(source.as_ref());
    let target_array = ArrayType::new(target.as_ref());
    let cast = context
        .explicit_cast(&source_array, &target_array)
        .unwrap()
        .into_resolution()
        .unwrap()
        .unwrap();
    assert_eq!(cast.context(), expected);
    assert!(cast.source().same_type(&source_array));
    assert!(cast.target().same_type(&target_array));
    assert_eq!(
        context
            .implicit_cast(&source_array, &target_array)
            .unwrap()
            .into_resolution()
            .unwrap()
            .is_some(),
        implicit
    );
    assert_eq!(
        context
            .assignment_cast(&source_array, &target_array)
            .unwrap()
            .into_resolution()
            .unwrap()
            .map(|cast| cast.context()),
        assignment
    );
}

#[test]
fn custom_enum_array_casts_and_candidates_follow_element_relationships() {
    let catalog = test_helpers::enum_cast_catalog(crate::ast::Dialect::postgres());
    let context = TypeContext::new(&catalog);
    let source = context
        .type_by_name(&FullName::with_schema("public", "source"))
        .unwrap()
        .unwrap();
    let target = context
        .type_by_name(&FullName::with_schema("public", "target"))
        .unwrap()
        .unwrap();
    let source_array = ArrayType::new(source.as_ref());
    let target_array = ArrayType::new(target.as_ref());
    let cast = context
        .implicit_cast(&source_array, &target_array)
        .unwrap()
        .into_resolution()
        .unwrap()
        .unwrap();
    assert_eq!(cast.context(), crate::types::CastContext::implicit());
    assert!(cast.source().same_type(&source_array));
    assert!(cast.target().same_type(&target_array));
    let expected = Candidates::from_candidates(&[
        Box::new(source_array.clone()),
        Box::new(target_array.clone()),
    ]);
    assert!(
        context
            .cast_sources(&target_array, false)
            .unwrap()
            .into_resolution()
            .unwrap()
            .same_type(expected.as_ref())
    );
    assert!(
        context
            .assignment_sources(&target_array)
            .unwrap()
            .into_resolution()
            .unwrap()
            .same_type(expected.as_ref())
    );
    assert!(
        context
            .implicit_castable(&source_array)
            .unwrap()
            .into_resolution()
            .unwrap()
            .same_type(expected.as_ref())
    );
    assert!(
        context
            .implicit_cast(&target_array, &source_array)
            .unwrap()
            .into_resolution()
            .unwrap()
            .is_none()
    );
}

#[cfg(feature = "sqlite")]
#[test]
fn sqlite_does_not_inherit_postgresql_element_array_casts() {
    let catalog = test_helpers::enum_cast_catalog(crate::ast::Dialect::sqlite());
    let context = TypeContext::new(&catalog);
    let source = context
        .type_by_name(&FullName::with_schema("public", "source"))
        .unwrap()
        .unwrap();
    let target = context
        .type_by_name(&FullName::with_schema("public", "target"))
        .unwrap()
        .unwrap();
    assert!(
        context
            .implicit_cast(
                &ArrayType::new(source.as_ref()),
                &ArrayType::new(target.as_ref())
            )
            .unwrap()
            .into_resolution()
            .unwrap()
            .is_none()
    );
}

#[test]
fn unregistered_table_row_array_identity_uses_loaded_components() {
    use crate::sem::{BaseParseContext, ParseContext, SchemaFileContext};
    use std::path::Path;
    let mut root = test_helpers::context();
    let row_reference = test_helpers::table_row(&mut root, false);
    let row = super::load_reference(&root, &row_reference).unwrap();
    let array = ArrayType::new(row.as_ref());
    let environment = test_helpers::context();
    let catalog = SchemaFileContext::new(
        Path::new("dynamic-array-casts.sql"),
        &root.freeze(),
        environment.get_args(),
        environment.metadata(),
    );
    let context = TypeContext::new(&catalog);
    assert!(
        context
            .implicit_cast(&array, &array)
            .unwrap()
            .into_resolution()
            .unwrap()
            .unwrap()
            .context()
            .is_no_conversion()
    );
    assert!(
        context
            .assignment_cast(&array, &array)
            .unwrap()
            .into_resolution()
            .unwrap()
            .unwrap()
            .context()
            .is_no_conversion()
    );
    assert!(
        context
            .explicit_cast(&array, &array)
            .unwrap()
            .into_resolution()
            .unwrap()
            .unwrap()
            .context()
            .is_no_conversion()
    );
}

#[rstest::rstest]
#[case::varchar("varchar")]
#[case::numeric("numeric")]
#[case::timestamp("timestamp")]
fn inherited_builtin_identity_remains_no_conversion(#[case] local: &str) {
    // Actual pg_cast self rows include typmod functions; same-type lookup remains identity.
    let catalog = test_helpers::heavy::builtin_context();
    let context = TypeContext::new(&catalog);
    let value = context
        .type_by_name(&FullName::with_schema("pg_catalog", local))
        .unwrap()
        .unwrap();
    let scalar = context
        .explicit_cast(value.as_ref(), value.as_ref())
        .unwrap()
        .into_resolution()
        .unwrap()
        .unwrap();
    assert!(scalar.context().is_no_conversion());
    let array = ArrayType::new(value.as_ref());
    let cast = context
        .explicit_cast(&array, &array)
        .unwrap()
        .into_resolution()
        .unwrap()
        .unwrap();
    assert!(cast.context().is_no_conversion());
    assert!(
        context
            .implicit_cast(&array, &array)
            .unwrap()
            .into_resolution()
            .unwrap()
            .unwrap()
            .context()
            .is_no_conversion()
    );
    assert!(
        context
            .assignment_cast(&array, &array)
            .unwrap()
            .into_resolution()
            .unwrap()
            .unwrap()
            .context()
            .is_no_conversion()
    );
}

#[test]
fn unregistered_complete_values_use_permanent_io_rules() {
    let mut catalog = test_helpers::context();
    let text_name = FullName::with_schema("pg_catalog", "text");
    catalog.set_get_type_impl_result(&text_name, &LegacyDefinition::builtin(&text_name));
    let context = TypeContext::new(&catalog);
    let text = context.type_by_exact_name(&text_name).unwrap().unwrap();
    let value = EnumType::new(
        &FullName::with_schema("unregistered", "state"),
        &["one".to_owned()],
    );
    let cast = context
        .assignment_cast(&value, text.as_ref())
        .unwrap()
        .into_resolution()
        .unwrap()
        .unwrap();
    assert_eq!(cast.context(), crate::types::CastContext::assignment());
    assert!(cast.source().same_type(&value));
    assert!(cast.target().same_type(text.as_ref()));
    let nested = ArrayType::new(&ArrayType::new(&value));
    let cast = context
        .assignment_cast(&nested, text.as_ref())
        .unwrap()
        .into_resolution()
        .unwrap()
        .unwrap();
    assert_eq!(cast.context(), crate::types::CastContext::assignment());
    assert!(cast.source().same_type(&nested));
    assert!(cast.target().same_type(text.as_ref()));
    assert!(
        context
            .implicit_cast(&value, text.as_ref())
            .unwrap()
            .into_resolution()
            .unwrap()
            .is_none()
    );
}

#[test]
fn unregistered_loaded_enum_retains_identity_in_all_candidate_directions() {
    use crate::types::TypeDefinition;
    let catalog = test_helpers::context();
    let context = TypeContext::new(&catalog);
    let value = EnumType::new(
        &FullName::with_schema("unregistered", "state"),
        &["one".to_owned()],
    );
    let cast = context
        .implicit_cast(&value, &value)
        .unwrap()
        .into_resolution()
        .unwrap()
        .unwrap();
    assert!(cast.context().is_no_conversion());
    assert!(cast.source().same_type(&value));
    assert!(cast.target().same_type(&value));
    let expected = Candidates::from_candidates(&[value.clone_box()]);
    assert!(
        context
            .cast_sources(&value, false)
            .unwrap()
            .into_resolution()
            .unwrap()
            .same_type(expected.as_ref())
    );
    assert!(
        context
            .cast_sources(&value, true)
            .unwrap()
            .into_resolution()
            .unwrap()
            .same_type(expected.as_ref())
    );
    assert!(
        context
            .assignment_sources(&value)
            .unwrap()
            .into_resolution()
            .unwrap()
            .same_type(expected.as_ref())
    );
    assert!(
        context
            .implicit_castable(&value)
            .unwrap()
            .into_resolution()
            .unwrap()
            .same_type(expected.as_ref())
    );
}
