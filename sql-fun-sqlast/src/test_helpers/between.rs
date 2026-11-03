use crate::{
    ast::Dialect,
    context::{BaseParseContext, MutSourcePathWithSpan, ParseContext, SchemaFileContext},
    names::{OperatorName, SchemaName},
    sem::{
        BinaryOperatorDefinition, OperatorDefinition, PgBuiltInType, SourcePathWithSpan,
        TypeCategory, TypeDefinition,
    },
    test_helpers::TestParseContext,
};

/// Small catalog with independently selected integer/text comparisons.
/// These tests need no builtin catalog loading or external services.
pub(crate) fn context(operators: &[&str]) -> SchemaFileContext {
    comparison_context(operators, false)
}

pub(crate) fn mixed_bounds_context(operators: &[&str]) -> SchemaFileContext {
    comparison_context(operators, true)
}

fn comparison_context(operators: &[&str], mixed_bounds: bool) -> SchemaFileContext {
    let mut base = TestParseContext::default().with_dialect(Dialect::postgres());
    base.set_get_search_path_result(&[SchemaName::from("public"), SchemaName::from("pg_catalog")]);
    for (ty, category) in [
        (PgBuiltInType::int4(), 'N'),
        (PgBuiltInType::bool(), 'B'),
        (PgBuiltInType::text(), 'S'),
    ] {
        base.setup_type(ty.clone());
        base.set_get_type_impl_result(
            ty.full_name().expect("nominal type identity"),
            &TypeDefinition::builtin_with_category(
                ty.full_name().expect("nominal type identity"),
                Some(TypeCategory::try_from(category).unwrap()),
                None,
            ),
        );
    }
    for operator in operators {
        let pairs = if mixed_bounds {
            [
                (PgBuiltInType::int4(), PgBuiltInType::bool()),
                (PgBuiltInType::int4(), PgBuiltInType::text()),
            ]
        } else {
            [
                (PgBuiltInType::int4(), PgBuiltInType::int4()),
                (PgBuiltInType::text(), PgBuiltInType::text()),
            ]
        };
        let signatures = pairs.map(|(left, right)| {
            BinaryOperatorDefinition::new(&left, &right, &PgBuiltInType::bool(), true)
        });
        base.set_get_operator_op_result(
            &OperatorName::new(operator),
            true,
            true,
            Some(&OperatorDefinition::Binary(signatures.to_vec())),
        );
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

pub(crate) fn schema(sql: &str) -> Result<SchemaFileContext, Box<dyn std::error::Error>> {
    let mut cache_context = crate::test_helpers::create_cache_context();
    let mut context = context(&[">=", "<=", "<", ">"]);
    context.set_source_path_with_span(&SourcePathWithSpan::new(
        "schema.sql",
        &crate::StringSpan::new(0, sql.len()),
    ));
    for statement in crate::parse_statements(sql, Dialect::postgres()) {
        context = statement?
            .analyze(context, &mut cache_context)?
            .into_context();
    }
    assert!(context.problems().is_empty(), "{:?}", context.problems());
    Ok(context)
}

pub(crate) fn query(
    sql: &str,
    context: SchemaFileContext,
) -> Result<(crate::sem::SelectStatement, SchemaFileContext), Box<dyn std::error::Error>> {
    let mut cache_context = crate::test_helpers::create_cache_context();
    let analyzed = crate::parse_statements(sql, Dialect::postgres())
        .next()
        .unwrap()?
        .analyze(context, &mut cache_context)?;
    let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
    Ok((query, analyzed.into_context()))
}

pub(crate) fn expression(query: &crate::sem::SelectStatement) -> &crate::sem::SemScalarExpr {
    query
        .result_columns()
        .get_column_at(0)
        .unwrap()
        .expression()
        .unwrap()
}

/// Inspect retained comparison lookups without repeating name resolution.
pub(crate) fn comparison_bindings(
    expression: &crate::sem::SemScalarExpr,
) -> Result<Vec<String>, crate::sem::AnalysisError> {
    let mut names = Bindings::inspect(expression)?.names;
    names.sort();
    Ok(names)
}

/// Types of each comparison's retained, coerced operand pair.
pub(crate) fn comparison_argument_types(
    expression: &crate::sem::SemScalarExpr,
) -> Result<Vec<[Option<crate::sem::TypeReference>; 2]>, crate::sem::AnalysisError> {
    Ok(Bindings::inspect(expression)?.argument_types)
}

#[derive(Default)]
struct Bindings {
    names: Vec<String>,
    argument_types: Vec<[Option<crate::sem::TypeReference>; 2]>,
}

impl Bindings {
    fn inspect(expression: &crate::sem::SemScalarExpr) -> Result<Self, crate::sem::AnalysisError> {
        let mut expression = expression.clone();
        let mut bindings = Self::default();
        expression.walk(&mut bindings)?;
        Ok(bindings)
    }
}

impl crate::sem::traversal::SemanticVisitorMut for Bindings {
    fn expression(
        &mut self,
        expression: &mut crate::sem::SemScalarExpr,
    ) -> Result<(), crate::sem::AnalysisError> {
        if let Some(binding) = expression.operator_binding() {
            assert!(binding.resolution().is_resolved());
            let encoded = ::serde_json::to_value(binding).unwrap();
            self.names.push(
                encoded["name"]["Valid"]["op_name"]
                    .as_str()
                    .unwrap()
                    .to_owned(),
            );
            self.argument_types.push(std::array::from_fn(|index| {
                if let Some(argument) = expression.binary_argument(index) {
                    argument.get_type()
                } else {
                    None
                }
            }));
        }
        Ok(())
    }
}
