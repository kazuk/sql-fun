use super::{SchemaCatalog, SchemaFileContext};
use crate::context::BaseParseContext;
use crate::sem::{ExtensionInfoRead, FullName, Kind, ObjectInfoRead};

impl ObjectInfoRead for SchemaCatalog {
    fn object_kind(&self, name: &FullName) -> Option<Kind> {
        let name = self.relation_identity(name);
        if let Some(kind) = self.objects.get(&name.to_string()) {
            Some(*kind)
        } else {
            self.base.object_kind(&name)
        }
    }
}
impl ExtensionInfoRead for SchemaCatalog {
    fn extension_exists(&self, name: &str) -> bool {
        self.extensions.contains(name) || self.base.extension_exists(name)
    }
}
impl SchemaCatalog {
    // SQLite folds ASCII identifiers even when quoted. Canonical Display still
    // encodes component boundaries after dialect-specific identity normalization.
    pub(super) fn relation_identity(&self, name: &FullName) -> FullName {
        if crate::ast::Dialect::from(self.metadata()) == crate::ast::Dialect::sqlite() {
            FullName::new(
                &name.catalog().map(|v| {
                    crate::sem::CatalogName::from(v.as_str().to_ascii_lowercase().as_str())
                }),
                &name.schema().map(|v| {
                    crate::sem::SchemaName::from(v.as_str().to_ascii_lowercase().as_str())
                }),
                &crate::sem::LocalName::from(name.name().as_str().to_ascii_lowercase().as_str()),
            )
        } else {
            name.clone()
        }
    }
    pub(super) fn store_table(&mut self, name: &FullName, definition: &crate::sem::CreateTable) {
        let name = self.relation_identity(name);
        self.tables.insert_create_table(&name, definition);
        self.objects.insert(&name.to_string(), Kind::table());
    }
    pub(super) fn store_view(
        &mut self,
        name: &FullName,
        definition: &crate::sem::ViewLike,
    ) -> Result<(), crate::sem::AnalysisError> {
        let name = self.relation_identity(name);
        let row_type =
            if crate::ast::Dialect::from(self.metadata()) == crate::ast::Dialect::postgres() {
                Some(crate::sem::CreateCompositeType::new(
                    &name,
                    &definition.result_columns().composite_field_collection()?,
                ))
            } else {
                None
            };
        self.views.insert(&name, definition);
        if let Some(row_type) = row_type {
            self.types.insert_create_composite_type(&name, &row_type);
        }
        let kind = if definition.is_materialized() {
            Kind::materialized_view()
        } else {
            Kind::view()
        };
        self.objects.insert(&name.to_string(), kind);
        Ok(())
    }
    pub(super) fn store_sequence(
        &mut self,
        name: &FullName,
        definition: &crate::sem::CreateSequence,
    ) {
        self.sequences.insert(name, definition);
        self.objects.insert(&name.to_string(), Kind::sequence());
    }
    pub(super) fn store_index(&mut self, definition: &crate::sem::CreateIndex) {
        self.indexes
            .insert(&definition.name().to_string(), definition.clone());
        self.objects
            .insert(&definition.name().to_string(), Kind::index());
    }
}
impl SchemaFileContext {
    pub(super) const fn creation_span(&self) -> crate::StringSpan {
        self.statement_span
    }
}
