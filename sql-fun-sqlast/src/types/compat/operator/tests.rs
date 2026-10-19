use crate::ast::Dialect;
use crate::sem::{
    BinaryOperatorDefinition, FullName, OperatorDefinition, OperatorName, PgBuiltInType,
    TypeReference as LegacyReference, UnaryOperatorDefinition,
};
use crate::types::{
    Candidates, Never, OperatorCall, OperatorForm, PolymorphicType, PseudoType, TypeContext,
    TypeDefinition, base::BaseType, compat::ConversionError,
};

mod test_helpers {
    use super::{
        BinaryOperatorDefinition, FullName, LegacyReference, OperatorDefinition, OperatorForm,
        OperatorName, PgBuiltInType, UnaryOperatorDefinition,
    };
    use crate::sem::{ColumnCollection, ColumnDefinition, TypeDefinition};
    use crate::test_helpers::TestParseContext;

    pub(super) fn context(dialect: crate::ast::Dialect) -> TestParseContext {
        let mut context = TestParseContext::default().with_dialect(dialect);
        context.setup_type(PgBuiltInType::int4());
        context.setup_type(PgBuiltInType::text());
        context
    }
    pub(super) fn reference(local: &str) -> LegacyReference {
        LegacyReference::concrete_type_ref(&FullName::with_schema("pg_catalog", local), false)
    }
    pub(super) fn record(member: Option<&LegacyReference>) -> LegacyReference {
        LegacyReference::dynamic(&TypeDefinition::from(&ColumnCollection::new(vec![
            ColumnDefinition::new(&None, member, Some(true), &[]),
        ])))
    }
    pub(super) fn register(
        context: &mut TestParseContext,
        name: &OperatorName,
        form: OperatorForm,
        operand: &LegacyReference,
        result: &LegacyReference,
    ) {
        let definition = if form == OperatorForm::binary() {
            OperatorDefinition::binary(&[BinaryOperatorDefinition::new(
                operand,
                &PgBuiltInType::text(),
                result,
                false,
            )])
        } else if form == OperatorForm::left_unary() {
            OperatorDefinition::left_unary(&[UnaryOperatorDefinition::new(operand, result, false)])
        } else {
            OperatorDefinition::right_unary(&[UnaryOperatorDefinition::new(operand, result, false)])
        };
        context.set_get_operator_op_result(
            name,
            form.has_left(),
            form.has_right(),
            Some(&definition),
        );
    }
    pub(super) mod heavy {
        use crate::sem::{BaseContext, FrozenBaseContext, SchemaFileContext};
        use ::clap::Parser;
        use ::sql_fun_core::SqlFunArgs;
        use std::path::Path;
        /// Loads the selected committed builtin catalog, including its full schema,
        /// once per call. Real extraction fidelity requires this costly fixture;
        /// no cache or service is shared across nextest processes.
        pub(in crate::types::compat::operator) fn builtin_context(
            version: &str,
        ) -> SchemaFileContext {
            let directory = ::tempfile::tempdir().unwrap();
            let metadata = directory.path().join("sql_fun.metadata.toml");
            std::fs::write(
                &metadata,
                format!(
                    r#"[package.metadata.database.default]
engine = "postgres"
dialect = "postgres"
version = "{version}"
search_path = ["public"]
"#
                ),
            )
            .unwrap();
            let args = SqlFunArgs::try_parse_from([
                "sqlfun",
                "--metadata-file",
                metadata.to_str().unwrap(),
                "--sql-fun-home",
                env!("SQL_FUN_HOME"),
            ])
            .unwrap();
            let collection = args.load_metadata().unwrap();
            let selected = collection.get("default").unwrap();
            let base = FrozenBaseContext::from(BaseContext::new(&args, selected).unwrap());
            SchemaFileContext::new(Path::new("operators.sql"), &base, &args, selected)
        }
    }
}

#[rstest::rstest]
fn legacy_reads_preserve_form_key_structure_and_strictness(
    #[values(
        OperatorForm::binary(),
        OperatorForm::left_unary(),
        OperatorForm::right_unary()
    )]
    form: OperatorForm,
    #[values(None, Some("public"), Some("a.b"), Some("a\"b"))] schema: Option<&str>,
) {
    let mut context = test_helpers::context(Dialect::postgres());
    let name = OperatorName::from_parts(schema, "~");
    let record = test_helpers::record(Some(&PgBuiltInType::int4()));
    test_helpers::register(&mut context, &name, form, &record, &record);
    let types = TypeContext::new(&context);
    let overloads = types.operator_by_name(&name, form).unwrap().unwrap();
    let signature = &overloads.signatures()[0];
    assert_eq!(signature.name(), &name);
    assert_eq!(signature.form(), form);
    assert!(!signature.is_strict());
    assert!(signature.result().same_type(signature.operand(0).unwrap()));
    assert!(
        signature
            .result()
            .as_any()
            .is::<crate::types::ImplicitCompositeType>()
    );
    assert_eq!(signature.left_operand().is_some(), form.has_left());
    assert_eq!(signature.right_operand().is_some(), form.has_right());
    if form == OperatorForm::binary() {
        assert!(
            signature
                .right_operand()
                .unwrap()
                .same_type(&BaseType::new(&FullName::with_schema("pg_catalog", "text")))
        );
    }
    assert!(
        types
            .operator_by_name(&OperatorName::from_parts(schema, "absent"), form)
            .unwrap()
            .is_none()
    );
}

#[rstest::rstest]
#[case::left_operand(0, "left_operand")]
#[case::right_operand(1, "right_operand")]
#[case::result(2, "result")]
fn incomplete_component_fails_the_entire_overload_list(#[case] index: usize, #[case] path: &str) {
    let mut context = test_helpers::context(Dialect::postgres());
    let name = OperatorName::from_parts(None, "~");
    let int = PgBuiltInType::int4();
    let mut references = [int.clone(), int.clone(), int.clone()];
    references[index] = test_helpers::record(None);
    let definition = OperatorDefinition::binary(&[
        BinaryOperatorDefinition::new(&int, &int, &int, true),
        BinaryOperatorDefinition::new(&references[0], &references[1], &references[2], false),
    ]);
    context.set_get_operator_op_result(&name, true, true, Some(&definition));
    let error = TypeContext::new(&context)
        .operator_by_name(&name, OperatorForm::binary())
        .unwrap_err();
    let expected = crate::types::compat::OperatorLoadError::new(
        &name,
        OperatorForm::binary(),
        1,
        path,
        &ConversionError::missing_member_type(0).to_string(),
    );
    assert_eq!(error, ConversionError::operator(&expected));
    assert_eq!(expected.name(), &name);
    assert_eq!(expected.form(), OperatorForm::binary());
    assert_eq!(expected.overload(), 1);
    assert_eq!(expected.component(), path);
    assert!(!expected.reason().is_empty());
}

#[test]
fn catalog_absence_empty_list_and_wrong_form_are_distinct() {
    let mut context = test_helpers::context(Dialect::postgres());
    let name = OperatorName::from_parts(None, "~");
    context.set_get_operator_op_result(&name, true, true, Some(&OperatorDefinition::binary(&[])));
    let empty = TypeContext::new(&context)
        .operator_by_name(&name, OperatorForm::binary())
        .unwrap()
        .unwrap();
    assert!(empty.is_empty());
    assert!(empty.candidates().as_any().is::<Never>());
    assert!(
        TypeContext::new(&context)
            .operator_by_name(&name, OperatorForm::left_unary())
            .unwrap()
            .is_none()
    );
    context.set_get_operator_op_result(
        &name,
        true,
        false,
        Some(&OperatorDefinition::right_unary(&[])),
    );
    assert!(
        TypeContext::new(&context)
            .operator_by_name(&name, OperatorForm::left_unary())
            .unwrap_err()
            .to_string()
            .contains("different operator form")
    );
}

#[test]
fn polymorphic_adaptation_retains_signature_scope_and_linkage() {
    let mut context = test_helpers::context(Dialect::postgres());
    let name = OperatorName::from_parts(None, "~");
    let element = test_helpers::reference("anyelement");
    let array = test_helpers::reference("anyarray");
    context.set_get_type_impl_result(
        element.full_name(),
        &crate::sem::TypeDefinition::builtin(element.full_name()),
    );
    context.set_get_type_impl_result(
        array.full_name(),
        &crate::sem::TypeDefinition::builtin(array.full_name()),
    );
    test_helpers::register(
        &mut context,
        &name,
        OperatorForm::right_unary(),
        &element,
        &array,
    );
    let read = || {
        TypeContext::new(&context)
            .operator_by_name(&name, OperatorForm::right_unary())
            .unwrap()
            .unwrap()
    };
    let overloads = read();
    let signature = &overloads.signatures()[0];
    assert!(signature.contains_polymorphic());
    assert!(signature.same_type(signature.clone_box().as_ref()));
    let input = signature
        .operand(0)
        .unwrap()
        .as_any()
        .downcast_ref::<PolymorphicType>()
        .unwrap();
    let output = signature
        .result()
        .as_any()
        .downcast_ref::<PolymorphicType>()
        .unwrap();
    assert_eq!(input.variable().array(), *output.variable());
    assert!(!signature.same_type(&read().signatures()[0]));
}

#[test]
fn declared_record_pseudo_type_does_not_claim_a_concrete_record_layout() {
    let mut context = test_helpers::context(Dialect::postgres());
    let name = OperatorName::from_parts(None, "=");
    let record = test_helpers::reference("record");
    test_helpers::register(
        &mut context,
        &name,
        OperatorForm::binary(),
        &record,
        &record,
    );
    let overloads = TypeContext::new(&context)
        .operator_by_name(&name, OperatorForm::binary())
        .unwrap()
        .unwrap();
    assert!(
        overloads.signatures()[0]
            .result()
            .as_any()
            .is::<PseudoType>()
    );
}

#[rstest::rstest]
#[case::postgres17("17")]
#[case::postgres18("18")]
fn committed_catalogs_preserve_numeric_unary_and_polymorphic_operators(#[case] version: &str) {
    // Full builtin ingestion is needed to verify the extractor/catalog boundary.
    let context = test_helpers::heavy::builtin_context(version);
    let types = TypeContext::new(&context);
    let name = OperatorName::from_parts(None, "-");
    let overloads = types
        .operator_by_name(&name, OperatorForm::binary())
        .unwrap()
        .unwrap();
    assert!(overloads.signatures().len() > 1);
    let int = BaseType::new(&FullName::with_schema("pg_catalog", "int4"));
    let small = BaseType::new(&FullName::with_schema("pg_catalog", "int2"));
    let matches = overloads.match_call(&OperatorCall::binary(&int, &small));
    assert_eq!(matches.overloads().signatures().len(), 1);
    let signature = &matches.overloads().signatures()[0];
    assert!(signature.left_operand().unwrap().same_type(&int));
    assert!(signature.right_operand().unwrap().same_type(&small));
    assert!(signature.result().same_type(&int));
    assert!(signature.is_strict());
    let prefix = types
        .operator_by_name(&name, OperatorForm::right_unary())
        .unwrap()
        .unwrap();
    assert_eq!(
        prefix
            .match_call(&OperatorCall::right_unary(&int))
            .overloads()
            .signatures()
            .len(),
        1
    );
    assert!(
        types
            .operator_by_name(&name, OperatorForm::left_unary())
            .unwrap()
            .is_none()
    );
    assert!(
        types
            .operator_by_name(
                &OperatorName::from_parts(Some("pg_catalog"), "-"),
                OperatorForm::binary()
            )
            .unwrap()
            .is_none()
    );
    let concat = types
        .operator_by_name(
            &OperatorName::from_parts(None, "||"),
            OperatorForm::binary(),
        )
        .unwrap()
        .unwrap();
    assert!(concat.signatures().iter().any(|s| s.contains_polymorphic()));
    let candidates = concat.result_candidates();
    assert!(candidates.as_any().is::<Candidates>());
}

#[cfg(feature = "sqlite")]
#[test]
fn sqlite_provider_does_not_infer_postgres_polymorphism() {
    let mut context = test_helpers::context(Dialect::sqlite());
    let name = OperatorName::from_parts(None, "~");
    let reference = test_helpers::reference("anyelement");
    context.set_get_type_impl_result(
        reference.full_name(),
        &crate::sem::TypeDefinition::builtin(reference.full_name()),
    );
    test_helpers::register(
        &mut context,
        &name,
        OperatorForm::right_unary(),
        &reference,
        &reference,
    );
    let error = TypeContext::new(&context)
        .operator_by_name(&name, OperatorForm::right_unary())
        .unwrap_err();
    let expected = crate::types::compat::OperatorLoadError::new(
        &name,
        OperatorForm::right_unary(),
        0,
        "right_operand",
        &ConversionError::unsupported_named_type(reference.full_name()).to_string(),
    );
    assert_eq!(error, ConversionError::operator(&expected));
}

#[test]
fn operand_type_names_resolve_and_array_components_remain_loaded() {
    let mut context = test_helpers::context(Dialect::postgres());
    context.set_get_search_path_result(&[crate::sem::SchemaName::from("pg_catalog")]);
    let name = OperatorName::from_parts(None, "~");
    let unqualified = LegacyReference::concrete_type_ref(&FullName::from_local_name("int4"), true);
    test_helpers::register(
        &mut context,
        &name,
        OperatorForm::right_unary(),
        &unqualified,
        &unqualified,
    );
    let overloads = TypeContext::new(&context)
        .operator_by_name(&name, OperatorForm::right_unary())
        .unwrap()
        .unwrap();
    let expected = crate::types::array::ArrayType::new(&BaseType::new(&FullName::with_schema(
        "pg_catalog",
        "int4",
    )));
    assert!(
        overloads.signatures()[0]
            .operand(0)
            .unwrap()
            .same_type(&expected)
    );
    assert!(overloads.signatures()[0].result().same_type(&expected));
}
