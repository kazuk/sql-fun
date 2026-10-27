use crate::ast::{Query, TryFromWithTokens};
use crate::sem::{AnalysisError, OperatorDefinition, OperatorName, PgBuiltInType};
use crate::syn::Opt;
use crate::test_helpers::{SynBuilder, TestParseContext};

use ::sql_fun_core::IVec;

pub(super) fn postgres(sql: &str) -> Result<Query, AnalysisError> {
    let statement = SynBuilder::new().parse(sql);
    Query::try_from_with_tokens(
        &statement.as_select_stmt().as_inner().expect("SELECT"),
        &IVec::default(),
    )
}

#[cfg(feature = "sqlite")]
pub(super) fn sqlite(sql: &str) -> Result<Query, AnalysisError> {
    let crate::syn::sqlite::Stmt::Select(query) =
        crate::test_helpers::SqliteSynBuilder.parse_as_stmt(sql)
    else {
        panic!("SELECT fixture");
    };
    Query::try_from(&query)
}

/// Lightweight common analysis environments.
pub(super) struct AnalysisFixture;

impl AnalysisFixture {
    pub(super) fn with_qualified_addition() -> TestParseContext {
        let mut catalog = Self::context();
        let integer = PgBuiltInType::int4();
        let definition = OperatorDefinition::binary(&[crate::sem::BinaryOperatorDefinition::new(
            &integer, &integer, &integer, true,
        )]);
        catalog.set_get_operator_op_result(
            &OperatorName::from_parts(Some("pg_catalog"), "+"),
            true,
            true,
            Some(&definition),
        );
        catalog
    }

    #[cfg(feature = "sqlite")]
    pub(super) fn with_integer_to_text_cast() -> TestParseContext {
        let mut catalog = Self::context();
        catalog.set_get_implicit_cast_result(
            &PgBuiltInType::int4(),
            &PgBuiltInType::text(),
            Some(crate::sem::CastDefinition::new(
                crate::sem::CastContext::implicit(),
            )),
        );
        catalog
    }
    pub(super) fn with_index_table() -> TestParseContext {
        let mut catalog = Self::context();
        catalog.set_get_search_path_result(&[crate::sem::SchemaName::from("public")]);
        catalog.setup_table("items", &[], &crate::test_helpers::source_span());
        catalog
    }

    pub(super) fn context() -> TestParseContext {
        let mut context = TestParseContext::default();
        context.setup_type(PgBuiltInType::int4());
        context.setup_type(PgBuiltInType::bool());
        context.setup_type(PgBuiltInType::text());
        context
    }
}
