mod ddl;

use crate::{
    context::ParseContext,
    names::{ColumnName, FullName, TableName},
    sem::{
        AnalysisError, CatalogNameLookup, ColumnCollection, ColumnDefinition, DeclarationCollation,
        Nullable, OwnerSpecification, TypeReference, WithTypmod, traversal::SemanticVisitor,
    },
};
use std::convert::Infallible;

/// collection for composite type field
#[derive(Debug, Default, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CompositeFieldCollection(Vec<CompositeFieldDefinition>);

impl CompositeFieldCollection {
    pub(crate) fn rename_relation(&mut self, old: &TableName, new: &TableName) {
        for field in &mut self.0 {
            if let Some(ty) = &mut field.field_type {
                ty.rename_relation(old, new);
            }
        }
    }

    pub(crate) fn declarations_can_apply(&self) -> bool {
        self.0
            .iter()
            .all(CompositeFieldDefinition::declaration_can_apply)
    }

    /// Append a field in declaration order.
    pub fn push(&mut self, item: CompositeFieldDefinition) {
        self.0.push(item);
    }

    /// get a table column collection
    pub fn get_column_collection(&self) -> ColumnCollection {
        let mut columns = Vec::new();
        for field in &self.0 {
            let col_def = ColumnDefinition::new(
                &Some(ColumnName::from(field.name.clone())),
                field.field_type.as_ref(),
                field.is_not_null,
                &[],
            );
            columns.push(col_def);
        }
        ColumnCollection::new(columns)
    }

    /// find member field by name
    pub fn get_member(&self, member: &str) -> Option<&CompositeFieldDefinition> {
        self.0.iter().find(|f| f.name == member)
    }
}

/// Analyzed CREATE TYPE AS statement.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CreateCompositeType {
    #[serde(skip)]
    owner: Option<OwnerSpecification>,
    type_ref: Option<TypeReference>,
    fields: CompositeFieldCollection,
}

impl CreateCompositeType {
    pub(crate) fn visit_references<V: SemanticVisitor>(
        &self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        self.fields
            .get_column_collection()
            .visit_references(visitor)
    }

    pub(crate) fn declarations_can_apply(&self) -> bool {
        self.fields.declarations_can_apply()
    }

    pub(crate) fn rename_relation(&mut self, old: &TableName, new: &TableName) {
        self.fields.rename_relation(old, new);
    }

    /// Retained ownership; None means no ownership information was supplied.
    /// Ownership is catalog state: it is omitted from the existing serialized
    /// type format and does not affect SQL type equality or hashing.
    pub const fn owner(&self) -> Option<&OwnerSpecification> {
        self.owner.as_ref()
    }
    /// Commit an ownership change, leaving the rest of the definition intact.
    pub(crate) fn change_owner(&mut self, owner: &OwnerSpecification) {
        self.owner = Some(owner.clone());
    }

    pub(crate) fn new(name: &FullName, fields: &CompositeFieldCollection) -> Self {
        Self {
            owner: None,
            type_ref: Some(TypeReference::from_full_name(name.clone())),
            fields: fields.clone(),
        }
    }

    /// name of composite type,
    ///
    /// When instance is dynamic type and no-name , returns None
    #[must_use]
    pub fn name(&self) -> Option<&FullName> {
        if let Some(reference) = &self.type_ref {
            reference.full_name()
        } else {
            None
        }
    }

    /// get concrete type reference
    ///
    /// When instance is dynamic type and no-named returns Name
    #[must_use]
    pub const fn type_ref(&self) -> &Option<TypeReference> {
        &self.type_ref
    }

    /// get a table column collection
    #[must_use]
    pub fn get_column_collection(&self) -> ColumnCollection {
        self.fields.get_column_collection()
    }

    /// find member field by name
    #[must_use]
    pub fn get_member(&self, member: &str) -> Option<&CompositeFieldDefinition> {
        self.fields.get_member(member)
    }

    /// create a dynamic composite type with field collection
    #[must_use]
    pub fn from_fields(fields: &CompositeFieldCollection) -> Self {
        Self {
            owner: None,
            type_ref: None,
            fields: fields.clone(),
        }
    }
}

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CompositeFieldDefinition {
    name: String,
    field_type: Option<TypeReference>,
    #[serde(default)]
    declaration_collation: DeclarationCollation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    requested_type: Option<Box<WithTypmod<TypeReference>>>,
    is_not_null: Option<Nullable>,
}

impl CompositeFieldDefinition {
    /// Requested declaration type and modifiers, separate from canonical identity.
    pub fn requested_type(&self) -> Option<&WithTypmod<TypeReference>> {
        self.requested_type.as_deref()
    }

    pub(crate) fn retain_requested_type(&mut self, value: &WithTypmod<TypeReference>) {
        self.requested_type = Some(Box::new(value.clone()));
    }

    /// Completed explicit field collation validation.
    pub const fn declaration_collation(&self) -> &DeclarationCollation {
        &self.declaration_collation
    }

    pub(crate) fn validate_collation(
        &mut self,
        context: &mut impl ParseContext,
        lookup: Option<&CatalogNameLookup>,
    ) -> Result<(), AnalysisError> {
        self.declaration_collation =
            DeclarationCollation::validate(context, self.field_type.as_ref(), lookup);
        self.declaration_collation
            .report(context, self.field_type.as_ref())
    }

    const fn declaration_can_apply(&self) -> bool {
        self.field_type.is_some() && self.declaration_collation.can_apply()
    }

    pub fn new(
        name: &str,
        field_type: &Option<TypeReference>,
        is_not_null: &Option<Nullable>,
    ) -> Self {
        Self {
            name: name.to_string(),
            field_type: field_type.clone(),
            declaration_collation: DeclarationCollation::default(),
            requested_type: None,
            is_not_null: *is_not_null,
        }
    }

    pub fn get_type(&self) -> Option<TypeReference> {
        self.field_type.clone()
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

// Ownership changes do not change SQL type identity or the existing serialized type format.
impl PartialEq for CreateCompositeType {
    fn eq(&self, other: &Self) -> bool {
        self.type_ref == other.type_ref && self.fields == other.fields
    }
}
impl Eq for CreateCompositeType {}
impl std::hash::Hash for CreateCompositeType {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::hash::Hash::hash(&self.type_ref, state);
        std::hash::Hash::hash(&self.fields, state);
    }
}

impl CreateCompositeType {
    /// Reject registration of an anonymous composite in the catalog.
    pub fn raise_registration_error(&self) -> Result<Infallible, AnalysisError> {
        AnalysisError::raise_unexpected_input(
            "anonymous composite cannot be registered in the catalog",
        )
    }
}
