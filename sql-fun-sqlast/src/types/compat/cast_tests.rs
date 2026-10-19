use super::{
    ConversionError, load_cast_definition, load_explicit_cast, load_implicit_cast, load_reference,
};
use crate::sem::{
    CastContext, CastDefinition, FullName, SchemaName, TypeDefinition, TypeReference,
};
use crate::types::{
    CompositeType, DomainType, EnumType, ImplicitCompositeType, TableRowType, array::ArrayType,
    base::BaseType,
};

mod test_helpers {
    use crate::sem::{
        ColumnCollection, ColumnDefinition, ColumnName, CompositeFieldCollection,
        CompositeFieldDefinition, CreateCompositeType, CreateDomain, CreateEnum, CreateTable,
        CreateTableMode, TableName,
    };
    use crate::sem::{FullName, SchemaName, TypeDefinition, TypeReference};
    use crate::test_helpers::TestParseContext;
    use crate::test_helpers::source_span;

    pub(super) fn context() -> TestParseContext {
        let mut context = TestParseContext::default().with_dialect(crate::ast::Dialect::postgres());
        context.set_get_search_path_result(&[SchemaName::from("public")]);
        context
    }

    pub(super) fn reference(local: &str) -> TypeReference {
        TypeReference::concrete_type_ref(&FullName::with_schema("public", local), false)
    }

    pub(super) fn install_base(context: &mut TestParseContext, local: &str) -> TypeReference {
        let reference = reference(local);
        context.set_get_type_impl_result(
            reference.full_name(),
            &TypeDefinition::builtin(reference.full_name()),
        );
        reference
    }

    pub(super) fn category(context: &mut TestParseContext, category: &str) -> TypeReference {
        let base = install_base(context, "integer");
        let named = reference("selected");
        let definition = match category {
            "base" => return base,
            "array" => return TypeReference::concrete_type_ref(base.full_name(), true),
            "enum" => TypeDefinition::from(&CreateEnum::new(
                named.full_name(),
                &["first".to_owned(), "second".to_owned()],
            )),
            "domain" => {
                TypeDefinition::from(&CreateDomain::new(&named, &Some(base.full_name().clone())))
            }
            "composite" => {
                let mut fields = CompositeFieldCollection::default();
                fields.push(CompositeFieldDefinition::new(
                    "value",
                    &Some(base.clone()),
                    &Some(true),
                ));
                TypeDefinition::from(&CreateCompositeType::new(named.full_name(), &fields))
            }
            "implicit" => {
                return TypeReference::dynamic(&TypeDefinition::from(&ColumnCollection::new(
                    vec![ColumnDefinition::new(
                        &Some(ColumnName::new("value")),
                        Some(&base),
                        Some(true),
                        &[],
                    )],
                )));
            }
            "row" => {
                let table_name = TableName::from(named.full_name().clone());
                let table = CreateTable::new(
                    &table_name,
                    vec![ColumnDefinition::new(
                        &Some(ColumnName::new("value")),
                        Some(&base),
                        Some(true),
                        &[],
                    )],
                    CreateTableMode::Default,
                    false,
                    &source_span(),
                );
                context.set_get_table_impl_result(named.full_name(), &Some(table));
                return TypeReference::dynamic(&TypeDefinition::table_ref(&table_name));
            }
            _ => panic!("unknown fixture category"),
        };
        context.set_get_type_impl_result(named.full_name(), &definition);
        named
    }
    pub(super) mod heavy {
        use crate::sem::{BaseContext, FrozenBaseContext, SchemaFileContext};
        use ::clap::Parser;
        use ::sql_fun_core::SqlFunArgs;
        use std::path::{Path, PathBuf};

        /// Loads real builtin metadata and cast catalogs once for each caller.
        /// Required to verify adaptation against existing catalog lookup semantics.
        pub(in crate::types::compat) fn builtin_context() -> SchemaFileContext {
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
            let selected = collection.get("default").expect("selected metadata");
            let base = BaseContext::new(&args, selected).expect("builtin context");
            let frozen = FrozenBaseContext::from(base);
            SchemaFileContext::new(Path::new("fixture.sql"), &frozen, &args, selected)
        }
    }
}

#[rstest::rstest]
#[case::implicit(CastContext::implicit(), crate::types::CastContext::implicit())]
#[case::preferred(
    CastContext::ImplicitPrefeerd,
    crate::types::CastContext::implicit_preferred()
)]
#[case::assignment(CastContext::assignment(), crate::types::CastContext::assignment())]
#[case::explicit(CastContext::explicit(), crate::types::CastContext::explicit())]
#[case::identity(
    CastContext::no_conversion(),
    crate::types::CastContext::no_conversion()
)]
fn preserves_each_legacy_context(
    #[case] context: CastContext,
    #[case] expected: crate::types::CastContext,
) {
    let mut catalog = test_helpers::context();
    let source = test_helpers::install_base(&mut catalog, "source");
    let target = if context.is_no_conversion() {
        source.clone()
    } else {
        test_helpers::install_base(&mut catalog, "target")
    };
    let loaded =
        load_cast_definition(&catalog, &source, &target, &CastDefinition::new(context)).unwrap();
    assert_eq!(loaded.context(), expected);
    assert!(
        loaded
            .source()
            .same_type(load_reference(&catalog, &source).unwrap().as_ref())
    );
    assert!(
        loaded
            .target()
            .same_type(load_reference(&catalog, &target).unwrap().as_ref())
    );
    assert_eq!(
        loaded.context().use_in_implicit(),
        context.use_in_implicit()
    );
    assert_eq!(
        loaded.context().use_in_assignment(),
        context.use_in_assignment()
    );
    assert_eq!(
        loaded.context().use_in_explicit(),
        context.use_in_explicit()
    );
    assert_eq!(
        loaded.context().is_implicit_preferred(),
        context.is_implicit_preferred()
    );
    assert_eq!(
        loaded.context().is_no_conversion(),
        context.is_no_conversion()
    );
}

#[rstest::rstest]
#[case::base("base")]
#[case::array("array")]
#[case::enumeration("enum")]
#[case::domain("domain")]
#[case::composite("composite")]
#[case::implicit_composite("implicit")]
#[case::table_row("row")]
fn loads_complete_endpoints_for_each_supported_category(#[case] category: &str) {
    let mut context = test_helpers::context();
    let reference = test_helpers::category(&mut context, category);
    let loaded = load_cast_definition(
        &context,
        &reference,
        &reference,
        &CastDefinition::new(CastContext::no_conversion()),
    )
    .unwrap();
    let expected = load_reference(&context, &reference).unwrap();
    assert!(loaded.source().same_type(expected.as_ref()));
    assert!(loaded.target().same_type(expected.as_ref()));
    let endpoint = loaded.source();
    match category {
        "base" => assert!(endpoint.as_any().is::<BaseType>()),
        "array" => assert!(endpoint.as_any().is::<ArrayType>()),
        "enum" => assert_eq!(
            endpoint
                .as_any()
                .downcast_ref::<EnumType>()
                .unwrap()
                .labels(),
            &["first", "second"]
        ),
        "domain" => assert!(
            endpoint
                .as_any()
                .downcast_ref::<DomainType>()
                .unwrap()
                .base_type()
                .as_any()
                .is::<BaseType>()
        ),
        "composite" => assert!(
            endpoint
                .as_any()
                .downcast_ref::<CompositeType>()
                .unwrap()
                .members()
                .member("value")
                .unwrap()
                .member_type()
                .as_any()
                .is::<BaseType>()
        ),
        "implicit" => assert!(
            endpoint
                .as_any()
                .downcast_ref::<ImplicitCompositeType>()
                .unwrap()
                .members()
                .member("value")
                .is_some()
        ),
        "row" => assert!(
            endpoint
                .as_any()
                .downcast_ref::<TableRowType>()
                .unwrap()
                .members()
                .member("value")
                .is_some()
        ),
        _ => unreachable!(),
    }
}

#[test]
fn resolves_unqualified_endpoints_before_directional_lookup() {
    let mut context = test_helpers::context();
    let source = test_helpers::install_base(&mut context, "source");
    let target = test_helpers::install_base(&mut context, "target");
    context.set_get_implicit_cast_result(
        &source,
        &target,
        Some(CastDefinition::new(CastContext::implicit())),
    );
    let local_source =
        TypeReference::concrete_type_ref(&FullName::from_local_name("source"), false);
    let local_target =
        TypeReference::concrete_type_ref(&FullName::from_local_name("target"), false);
    let loaded = load_implicit_cast(&context, &local_source, &local_target)
        .unwrap()
        .unwrap();
    assert!(
        loaded
            .source()
            .same_type(load_reference(&context, &source).unwrap().as_ref())
    );
    assert!(
        loaded
            .target()
            .same_type(load_reference(&context, &target).unwrap().as_ref())
    );
    assert!(
        load_implicit_cast(&context, &local_target, &local_source)
            .unwrap()
            .is_none()
    );
    assert!(
        load_explicit_cast(&context, &local_target, &local_source)
            .unwrap()
            .is_none()
    );
}

#[rstest::rstest]
fn missing_endpoint_is_an_error(#[values(false, true)] missing_source: bool) {
    let mut context = test_helpers::context();
    let known = test_helpers::install_base(&mut context, "known");
    let missing = test_helpers::reference("missing");
    let (source, target) = if missing_source {
        (&missing, &known)
    } else {
        (&known, &missing)
    };
    assert_eq!(
        load_cast_definition(
            &context,
            source,
            target,
            &CastDefinition::new(CastContext::explicit())
        )
        .unwrap_err(),
        ConversionError::type_not_found(missing.full_name())
    );
}

#[rstest::rstest]
#[case::range("int4range")]
#[case::multirange("int4multirange")]
#[case::polymorphic("anyelement")]
#[case::pseudo("void")]
fn identity_does_not_fabricate_missing_category_components(#[case] local: &str) {
    let mut context = test_helpers::context();
    let name = FullName::with_schema("pg_catalog", local);
    context.set_get_type_impl_result(&name, &TypeDefinition::builtin(&name));
    let reference = TypeReference::concrete_type_ref(&name, false);
    assert_eq!(
        load_cast_definition(
            &context,
            &reference,
            &reference,
            &CastDefinition::new(CastContext::no_conversion())
        )
        .unwrap_err(),
        ConversionError::unsupported_named_type(&name)
    );
}

#[test]
fn propagates_incomplete_and_recursive_domain_errors() {
    use crate::sem::CreateDomain;
    let mut context = test_helpers::context();
    let reference = test_helpers::reference("domain");
    let incomplete = TypeDefinition::from(&CreateDomain::new(&reference, &None));
    context.set_get_type_impl_result(reference.full_name(), &incomplete);
    assert_eq!(
        load_cast_definition(
            &context,
            &reference,
            &reference,
            &CastDefinition::new(CastContext::no_conversion())
        )
        .unwrap_err(),
        ConversionError::missing_domain_base(reference.full_name())
    );
    let recursive = TypeDefinition::from(&CreateDomain::new(
        &reference,
        &Some(reference.full_name().clone()),
    ));
    context.set_get_type_impl_result(reference.full_name(), &recursive);
    assert_eq!(
        load_cast_definition(
            &context,
            &reference,
            &reference,
            &CastDefinition::new(CastContext::no_conversion())
        )
        .unwrap_err(),
        ConversionError::recursive_type(reference.full_name())
    );
}

#[rstest::rstest]
#[case::implicit("int2", "int4")]
#[case::assignment("int4", "int2")]
#[case::explicit("text", "xml")]
#[case::identity("int4", "int4")]
fn adapts_existing_builtin_casts_without_changing_eligibility(
    #[case] source: &str,
    #[case] target: &str,
) {
    use super::load_assignment_cast;
    use crate::sem::CastInfoRead;
    // Each case loads real metadata to compare adapters with catalog results.
    let context = test_helpers::heavy::builtin_context();
    let source = TypeReference::from_master_type_name(source);
    let target = TypeReference::from_master_type_name(target);
    let legacy = context.get_explicit_cast(&source, &target);
    let loaded = load_explicit_cast(&context, &source, &target).unwrap();
    assert_eq!(loaded.is_some(), legacy.is_some());
    if let (Some(loaded), Some(legacy)) = (loaded, legacy) {
        assert_eq!(
            loaded.context(),
            crate::types::CastContext::from(legacy.context())
        );
        assert!(
            loaded
                .source()
                .same_type(load_reference(&context, &source).unwrap().as_ref())
        );
        assert!(
            loaded
                .target()
                .same_type(load_reference(&context, &target).unwrap().as_ref())
        );
    }
    assert_eq!(
        load_implicit_cast(&context, &source, &target)
            .unwrap()
            .is_some(),
        context.get_implicit_cast(&source, &target).is_some()
    );
    assert_eq!(
        load_assignment_cast(&context, &source, &target)
            .unwrap()
            .is_some(),
        context.get_assignment_cast(&source, &target).is_some()
    );
}

#[rstest::rstest]
fn candidate_inputs_remain_distinct_from_target(#[values(false, true)] explicit: bool) {
    use super::{load_assignment_sources, load_cast_sources};
    use crate::types::Candidates;
    let mut context = test_helpers::context();
    let source = test_helpers::category(&mut context, "enum");
    let target = test_helpers::install_base(&mut context, "target");
    context.set_get_implicit_cast_result(
        &source,
        &target,
        Some(CastDefinition::new(CastContext::implicit())),
    );
    let target_value = load_reference(&context, &target).unwrap();
    let expected = Candidates::from_candidates(&[
        load_reference(&context, &source).unwrap(),
        target_value.clone(),
    ]);
    // This lightweight catalog deliberately ignores explicit mode; both choices
    // must still preserve the exact complete source values it enumerates.
    let candidates = load_cast_sources(&context, &target, explicit).unwrap();
    assert!(candidates.same_type(expected.as_ref()));
    assert!(!candidates.same_type(target_value.as_ref()));
    assert!(
        load_assignment_sources(&context, &target)
            .unwrap()
            .same_type(expected.as_ref())
    );
}

#[test]
fn assignment_sources_exclude_explicit_only_candidates() {
    use super::{load_assignment_sources, load_cast_sources};
    use crate::types::Candidates;
    let mut context = test_helpers::context();
    let source = test_helpers::install_base(&mut context, "source");
    let target = test_helpers::install_base(&mut context, "target");
    context.set_get_implicit_cast_result(
        &source,
        &target,
        Some(CastDefinition::new(CastContext::explicit())),
    );
    let explicit = load_cast_sources(&context, &target, true).unwrap();
    let expected_explicit = Candidates::from_candidates(&[
        load_reference(&context, &source).unwrap(),
        load_reference(&context, &target).unwrap(),
    ]);
    assert!(explicit.same_type(expected_explicit.as_ref()));
    let assignment = load_assignment_sources(&context, &target).unwrap();
    let expected_assignment =
        Candidates::from_candidates(&[load_reference(&context, &target).unwrap()]);
    assert!(assignment.same_type(expected_assignment.as_ref()));
}

#[test]
fn missing_candidate_prevents_partial_or_empty_success() {
    use super::{load_assignment_sources, load_cast_sources};
    let mut context = test_helpers::context();
    let known = test_helpers::install_base(&mut context, "known");
    let target = test_helpers::install_base(&mut context, "target");
    let missing = test_helpers::reference("missing");
    context.set_get_implicit_cast_result(
        &known,
        &target,
        Some(CastDefinition::new(CastContext::implicit())),
    );
    context.set_get_implicit_cast_result(
        &missing,
        &target,
        Some(CastDefinition::new(CastContext::implicit())),
    );
    let expected = ConversionError::type_not_found(missing.full_name());
    assert_eq!(
        load_cast_sources(&context, &target, false).unwrap_err(),
        expected
    );
    assert_eq!(
        load_cast_sources(&context, &target, true).unwrap_err(),
        expected
    );
    assert_eq!(
        load_assignment_sources(&context, &target).unwrap_err(),
        expected
    );
}

#[test]
fn search_path_shadowing_preserves_explicit_schema_choices() {
    let mut context = test_helpers::context();
    let public_source = test_helpers::install_base(&mut context, "source");
    let other_name = FullName::with_schema("other", "source");
    context.set_get_type_impl_result(&other_name, &TypeDefinition::builtin(&other_name));
    let other_source = TypeReference::concrete_type_ref(&other_name, false);
    let target = test_helpers::install_base(&mut context, "target");
    context.set_get_implicit_cast_result(
        &public_source,
        &target,
        Some(CastDefinition::new(CastContext::implicit())),
    );
    let local_source =
        TypeReference::concrete_type_ref(&FullName::from_local_name("source"), false);
    assert!(
        load_implicit_cast(&context, &local_source, &target)
            .unwrap()
            .is_some()
    );
    context.set_get_search_path_result(&[SchemaName::from("other"), SchemaName::from("public")]);
    assert!(
        load_implicit_cast(&context, &local_source, &target)
            .unwrap()
            .is_none()
    );
    assert!(
        load_implicit_cast(&context, &other_source, &target)
            .unwrap()
            .is_none()
    );
    let explicit_public = load_implicit_cast(&context, &public_source, &target)
        .unwrap()
        .unwrap();
    assert!(
        explicit_public
            .source()
            .same_type(load_reference(&context, &public_source).unwrap().as_ref())
    );
}

#[test]
fn array_lookup_preserves_endpoint_qualification() {
    let mut context = test_helpers::context();
    let scalar_source = test_helpers::install_base(&mut context, "source");
    let scalar_target = test_helpers::install_base(&mut context, "target");
    let array_source = TypeReference::concrete_type_ref(scalar_source.full_name(), true);
    let array_target = TypeReference::concrete_type_ref(scalar_target.full_name(), true);
    context.set_get_implicit_cast_result(
        &array_source,
        &array_target,
        Some(CastDefinition::new(CastContext::implicit())),
    );
    let loaded = load_implicit_cast(&context, &array_source, &array_target)
        .unwrap()
        .unwrap();
    assert!(loaded.source().as_any().is::<ArrayType>());
    assert!(loaded.target().as_any().is::<ArrayType>());
    assert!(
        load_implicit_cast(&context, &scalar_source, &scalar_target)
            .unwrap()
            .is_none()
    );
    assert!(
        load_implicit_cast(&context, &array_source, &scalar_target)
            .unwrap()
            .is_none()
    );
}
