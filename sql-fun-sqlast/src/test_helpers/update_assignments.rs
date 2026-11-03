use crate::test_helpers::update::catalog as test_helpers_catalog;

use crate::ast::Dialect;
use crate::names::ColumnName;
use crate::sem::{ColumnDefinition, PgBuiltInType};
use crate::test_helpers::{TestParseContext, source_span};

pub(crate) fn catalog() -> TestParseContext {
    let mut context = test_helpers_catalog(Dialect::postgres());
    let array = PgBuiltInType::int4().array_type();
    context.setup_type(array.clone());
    context.setup_table(
        "arrays",
        &[ColumnDefinition::new(
            &Some(ColumnName::from("a")),
            Some(&array),
            Some(false),
            &[],
        )],
        &source_span(),
    );
    context
}
