use crate::sem::resolution::Catalog;
use crate::sem::unresolved::{Query, TryFromWithTokens};
use crate::sem::{
    AnalysisError, CastDefinition, CreateTable, FullName, FunctionOverloadCollection,
    OperatorDefinition, OperatorName, PgBuiltInType, TableName, TypeDefinition, TypeReference,
    ViewLike, ViewName,
};
use crate::syn::Opt;
use crate::test_helpers::{SynBuilder, TestParseContext};

use std::collections::HashSet;

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

/// Deliberately implements no ParseContext or catalog-write interface.
pub(super) struct ReadOnlyCatalog(TestParseContext);

impl ReadOnlyCatalog {
    pub(super) fn with_qualified_addition() -> Self {
        let mut catalog = Self::new();
        let integer = PgBuiltInType::int4();
        let definition = OperatorDefinition::binary(&[crate::sem::BinaryOperatorDefinition::new(
            &integer, &integer, &integer, true,
        )]);
        catalog.0.set_get_operator_op_result(
            &OperatorName::from_parts(Some("pg_catalog"), "+"),
            true,
            true,
            Some(&definition),
        );
        catalog
    }

    #[cfg(feature = "sqlite")]
    pub(super) fn with_integer_to_text_cast() -> Self {
        let mut catalog = Self::new();
        catalog.0.set_get_implicit_cast_result(
            &PgBuiltInType::int4(),
            &PgBuiltInType::text(),
            Some(CastDefinition::new(crate::sem::CastContext::implicit())),
        );
        catalog
    }
    pub(super) fn new() -> Self {
        let mut context = TestParseContext::default();
        context.setup_type(PgBuiltInType::int4());
        context.setup_type(PgBuiltInType::bool());
        context.setup_type(PgBuiltInType::text());
        Self(context)
    }
}

impl Catalog for ReadOnlyCatalog {
    fn resolve_function_name(&self, name: &FullName) -> FullName {
        Catalog::resolve_function_name(&self.0, name)
    }
    fn get_type(&self, name: &FullName) -> Option<&TypeDefinition> {
        Catalog::get_type(&self.0, name)
    }
    fn get_table(&self, name: &TableName) -> Option<&CreateTable> {
        Catalog::get_table(&self.0, name)
    }
    fn get_view(&self, name: &ViewName) -> Option<&ViewLike> {
        Catalog::get_view(&self.0, name)
    }
    fn get_function_by_name(&self, name: &FullName) -> Option<FunctionOverloadCollection> {
        Catalog::get_function_by_name(&self.0, name)
    }
    fn get_operator_op(
        &self,
        name: &OperatorName,
        left: bool,
        right: bool,
    ) -> Option<&OperatorDefinition> {
        Catalog::get_operator_op(&self.0, name, left, right)
    }
    fn get_array_item_type(&self, name: &TypeReference) -> Option<TypeReference> {
        Catalog::get_array_item_type(&self.0, name)
    }
    fn get_explicit_cast(
        &self,
        source: &TypeReference,
        target: &TypeReference,
    ) -> Option<CastDefinition> {
        Catalog::get_explicit_cast(&self.0, source, target)
    }
    fn get_implicit_cast(
        &self,
        source: &TypeReference,
        target: &TypeReference,
    ) -> Option<CastDefinition> {
        Catalog::get_implicit_cast(&self.0, source, target)
    }
    fn get_implicit_castable(&self, source: &TypeReference) -> HashSet<TypeReference> {
        Catalog::get_implicit_castable(&self.0, source)
    }
    fn canonicalize_type_ref(&self, value: &TypeReference) -> Option<TypeReference> {
        Catalog::canonicalize_type_ref(&self.0, value)
    }
}
