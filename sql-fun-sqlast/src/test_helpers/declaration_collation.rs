//! Small catalog for declaration validation and schema-chain tests.

use crate::{
    ast::Dialect,
    context::{MutSourcePathWithSpan, ParseContext, SchemaFileContext},
    names::{FullName, SchemaName},
    sem::{AnalysisError, DeclarationCollation, SemAst, TypeDefinition, TypeReference, WithTypmod},
    test_helpers::{collate, conflict_masters::Masters, schema_layer, source_span},
};

pub(crate) fn context(fact: Option<bool>) -> SchemaFileContext {
    let mut base = collate::context(Dialect::postgres());
    let name = FullName::with_schema("public", "custom.type");
    base.set_get_type_impl_result(&name, &TypeDefinition::builtin(&name, fact));
    let mut masters = Masters::new();
    masters.add_collation("collation.schema", "Mixed.Name", -1, "c");
    base.set_conflict_masters(&masters.load().unwrap());
    let varchar = TypeReference::from_master_type_name("varchar");
    base.set_get_type_impl_result(
        varchar.full_name().expect("nominal type identity"),
        &TypeDefinition::builtin(
            varchar.full_name().expect("nominal type identity"),
            Some(true),
        ),
    );
    base.set_search_path(&[SchemaName::from("public")]);
    let mut context = schema_layer(
        std::path::Path::new("declaration-collation.sql"),
        &base.freeze(),
    );
    context.set_source_path_with_span(&source_span());
    context
}

impl SemAst {
    pub(crate) fn apply_declaration(
        &self,
        context: SchemaFileContext,
    ) -> Result<SchemaFileContext, AnalysisError> {
        match self {
            Self::CreateTable(table) => context.apply_create_table(table),
            Self::CreateCompType(composite) => context.apply_create_comp_type(composite),
            Self::CreateDomain(domain) => context.apply_create_domain(domain),
            _ => panic!("expected a declaration"),
        }
    }

    pub(crate) fn first_declaration_collation(&self) -> &DeclarationCollation {
        match self {
            Self::CreateTable(table) => table.get_column_at(0).unwrap().declaration_collation(),
            Self::CreateCompType(composite) => composite
                .get_member("value")
                .unwrap()
                .declaration_collation(),
            Self::CreateDomain(domain) => domain.declaration_collation(),
            _ => panic!("expected a declaration"),
        }
    }

    pub(crate) fn first_requested_type(&self) -> &WithTypmod<TypeReference> {
        match self {
            Self::CreateTable(table) => table.get_column_at(0).unwrap().requested_type().unwrap(),
            Self::CreateCompType(composite) => composite
                .get_member("value")
                .unwrap()
                .requested_type()
                .unwrap(),
            Self::CreateDomain(domain) => domain.requested_base().unwrap(),
            _ => panic!("expected a declaration"),
        }
    }
}
