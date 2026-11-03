use crate::ast::{Query, TryFromWithTokens};
use crate::names::{OperatorName, SchemaName};
#[cfg(feature = "sqlite")]
use crate::sem::CastContext;
#[cfg(feature = "sqlite")]
use crate::sem::CastDefinition;
use crate::sem::{AnalysisError, BinaryOperatorDefinition};
use crate::sem::{OperatorDefinition, PgBuiltInType};
use crate::syn::Opt;
#[cfg(feature = "sqlite")]
use crate::syn::sqlite::Stmt;
#[cfg(feature = "sqlite")]
use crate::test_helpers::SqliteSynBuilder;
use crate::test_helpers::{SynBuilder, TestParseContext, source_span};

use ::sql_fun_core::IVec;

pub(crate) fn postgres(sql: &str) -> Result<Query, AnalysisError> {
    let statement = SynBuilder::new().parse(sql);
    Query::try_from_with_tokens(
        &statement.as_select_stmt().as_inner().expect("SELECT"),
        &IVec::default(),
    )
}

#[cfg(feature = "sqlite")]
pub(crate) fn sqlite(sql: &str) -> Result<Query, AnalysisError> {
    let Stmt::Select(query) = SqliteSynBuilder.parse_as_stmt(sql) else {
        panic!("SELECT fixture");
    };
    Query::try_from(&query)
}

/// Lightweight common analysis environments.
pub(crate) struct AnalysisFixture;

impl AnalysisFixture {
    pub(crate) fn with_qualified_addition() -> TestParseContext {
        let mut catalog = Self::context();
        let integer = PgBuiltInType::int4();
        let definition = OperatorDefinition::binary(&[BinaryOperatorDefinition::new(
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
    pub(crate) fn with_integer_to_text_cast() -> TestParseContext {
        let mut catalog = Self::context();
        catalog.set_get_implicit_cast_result(
            &PgBuiltInType::int4(),
            &PgBuiltInType::text(),
            Some(CastDefinition::new(CastContext::implicit())),
        );
        catalog
    }
    pub(crate) fn with_index_table() -> TestParseContext {
        let mut catalog = Self::context();
        catalog.set_get_search_path_result(&[SchemaName::from("public")]);
        catalog.setup_table("items", &[], &source_span());
        catalog
    }

    pub(crate) fn context() -> TestParseContext {
        let mut context = TestParseContext::default();
        context.setup_type(PgBuiltInType::int4());
        context.setup_type(PgBuiltInType::bool());
        context.setup_type(PgBuiltInType::text());
        context
    }
}
