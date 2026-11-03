use crate::ast::Dialect;
use crate::test_helpers::scalar_catalog::{categorized_scalar_catalog, schema_from_base};
use crate::test_helpers::statements::schema_postgres;

use crate::context::SchemaFileContext;
use crate::names::OperatorName;
use crate::sem::{BinaryOperatorDefinition, OperatorDefinition};

use crate::sem::PgBuiltInType;

/// Small catalog for distinctness binding and inference tests.
/// These tests need no builtin catalog loading or external services.
pub(crate) fn context() -> SchemaFileContext {
    context_for(Dialect::postgres())
}

pub(crate) fn context_for(dialect: Dialect) -> SchemaFileContext {
    let mut base = categorized_scalar_catalog(dialect);

    let definitions = [
        PgBuiltInType::int4(),
        PgBuiltInType::text(),
        PgBuiltInType::bool(),
    ]
    .map(|ty| BinaryOperatorDefinition::new(&ty, &ty, &PgBuiltInType::bool(), true));
    base.set_get_operator_op_result(
        &OperatorName::from_parts(None, "="),
        true,
        true,
        Some(&OperatorDefinition::binary(&definitions)),
    );
    schema_from_base(base, std::path::Path::new("query.sql"))
}

pub(crate) fn schema(sql: &str) -> Result<SchemaFileContext, Box<dyn std::error::Error>> {
    schema_postgres(sql, context())
}
