use crate::context::{BaseParseContext, ParseContext, SchemaFileContext};
use crate::sem::{
    BinaryOperatorDefinition, FullName, FunctionOverloadCollection, FunctionParam,
    OperatorDefinition, OperatorName, OverloadVariant, PgBuiltInType, SchemaName, TypeCategory,
    TypeDefinition,
};
use crate::test_helpers::TestParseContext;

pub(crate) use crate::test_helpers::membership::{expression, query};

/// A finite pattern catalog for binding and correlated parameter contracts.
/// Actual builtin completeness is covered by the maintained CLI fixtures.
pub(crate) fn context() -> SchemaFileContext {
    let mut base = TestParseContext::default();
    base.set_get_search_path_result(&[SchemaName::from("public"), SchemaName::from("pg_catalog")]);
    for (ty, category) in [
        (PgBuiltInType::text(), 'S'),
        (
            crate::sem::TypeReference::from_master_type_name("bytea"),
            'U',
        ),
        (PgBuiltInType::int4(), 'N'),
        (PgBuiltInType::bool(), 'B'),
    ] {
        base.setup_type(ty.clone());
        base.set_get_type_impl_result(
            ty.full_name(),
            &TypeDefinition::builtin_with_category(
                ty.full_name(),
                Some(TypeCategory::try_from(category).unwrap()),
                None,
            ),
        );
    }
    for name in ["~~", "!~~", "~~*", "!~~*", "~", "!~"] {
        let mut signatures = vec![BinaryOperatorDefinition::new(
            &PgBuiltInType::text(),
            &PgBuiltInType::text(),
            &PgBuiltInType::bool(),
            true,
        )];
        if matches!(name, "~~" | "!~~") {
            signatures.push(BinaryOperatorDefinition::new(
                &crate::sem::TypeReference::from_master_type_name("bytea"),
                &crate::sem::TypeReference::from_master_type_name("bytea"),
                &PgBuiltInType::bool(),
                true,
            ));
        }
        base.set_get_operator_op_result(
            &OperatorName::from_parts(None, name),
            true,
            true,
            Some(&OperatorDefinition::binary(&signatures)),
        );
    }
    for name in ["like_escape", "similar_to_escape"] {
        let signatures = if name == "like_escape" {
            vec![
                helper_signature(&PgBuiltInType::text(), 2),
                helper_signature(
                    &crate::sem::TypeReference::from_master_type_name("bytea"),
                    2,
                ),
            ]
        } else {
            vec![
                helper_signature(&PgBuiltInType::text(), 1),
                helper_signature(&PgBuiltInType::text(), 2),
            ]
        };
        let name = FullName::with_schema("pg_catalog", name);
        for signature in &signatures {
            base.set_function_returns_set(&name, signature, false);
            base.set_function_row_property(&name, signature, true);
        }
        base.set_get_function_impl_result(&name, &FunctionOverloadCollection::new(&signatures));
    }
    let args = base.get_args().clone();
    let metadata = base.metadata().clone();
    SchemaFileContext::new(
        std::path::Path::new("query.sql"),
        &base.freeze(),
        &args,
        &metadata,
    )
}

fn helper_signature(ty: &crate::sem::TypeReference, count: usize) -> OverloadVariant {
    OverloadVariant::new(
        &Some(ty.clone()),
        &vec![FunctionParam::new_input_param(&None, &Some(ty.clone()), &None); count],
        true,
        false,
    )
}
