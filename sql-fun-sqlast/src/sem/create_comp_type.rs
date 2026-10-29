use crate::{
    context::ParseContext,
    sem::{AstAndContextPair, ColumnDefinition, ColumnName, Nullable},
    syn::{ColumnDef, Opt},
};

use super::{AnalysisError, ColumnCollection, FullName, TypeReference};

/// collection for composite type field
#[derive(Debug, Default, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CompositeFieldCollection(Vec<CompositeFieldDefinition>);

impl CompositeFieldCollection {
    pub(crate) fn rename_relation(
        &mut self,
        old: &crate::sem::TableName,
        new: &crate::sem::TableName,
    ) {
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

/// analyzed [`crate::syn::CompositeTypeStmt`]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CreateCompositeType {
    #[serde(skip)]
    owner: Option<crate::sem::OwnerSpecification>,
    type_ref: Option<TypeReference>,
    fields: CompositeFieldCollection,
}

impl CreateCompositeType {
    pub(crate) fn declarations_can_apply(&self) -> bool {
        self.fields.declarations_can_apply()
    }

    pub(crate) fn rename_relation(
        &mut self,
        old: &crate::sem::TableName,
        new: &crate::sem::TableName,
    ) {
        self.fields.rename_relation(old, new);
    }

    /// Retained ownership; None means no ownership information was supplied.
    /// Ownership is catalog state: it is omitted from the existing serialized
    /// type format and does not affect SQL type equality or hashing.
    pub const fn owner(&self) -> Option<&crate::sem::OwnerSpecification> {
        self.owner.as_ref()
    }
    /// Commit an ownership change, leaving the rest of the definition intact.
    pub(crate) fn change_owner(&mut self, owner: &crate::sem::OwnerSpecification) {
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
        self.type_ref.as_ref().map(|f| f.full_name())
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

    /// analyze [`crate::syn::CompositeTypeStmt`]
    #[tracing::instrument(skip(context, _parent_schema, syn))]
    pub fn analyze_create_comp_type<TParseContext>(
        context: TParseContext,
        _parent_schema: &Option<String>,
        syn: crate::syn::CompositeTypeStmt,
    ) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        use crate::ast::TryFromWithTokens;
        crate::ast::CompositeType::try_from_with_tokens(&syn, &sql_fun_core::IVec::default())?
            .resolve_and_apply(context)
    }
}

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CompositeFieldDefinition {
    name: String,
    field_type: Option<TypeReference>,
    #[serde(default)]
    declaration_collation: crate::sem::DeclarationCollation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    requested_type: Option<Box<crate::sem::WithTypmod<TypeReference>>>,
    is_not_null: Option<Nullable>,
}

impl CompositeFieldDefinition {
    /// Requested declaration type and modifiers, separate from canonical identity.
    pub fn requested_type(&self) -> Option<&crate::sem::WithTypmod<TypeReference>> {
        self.requested_type.as_deref()
    }

    pub(crate) fn retain_requested_type(&mut self, value: &crate::sem::WithTypmod<TypeReference>) {
        self.requested_type = Some(Box::new(value.clone()));
    }

    /// Completed explicit field collation validation.
    pub const fn declaration_collation(&self) -> &crate::sem::DeclarationCollation {
        &self.declaration_collation
    }

    pub(crate) fn validate_collation(
        &mut self,
        context: &mut impl ParseContext,
        lookup: Option<&crate::sem::CatalogNameLookup>,
    ) -> Result<(), AnalysisError> {
        self.declaration_collation =
            crate::sem::DeclarationCollation::validate(context, self.field_type.as_ref(), lookup);
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
            declaration_collation: crate::sem::DeclarationCollation::default(),
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

impl TryFrom<&ColumnDef> for CompositeFieldDefinition {
    type Error = AnalysisError;

    fn try_from(value: &ColumnDef) -> Result<Self, Self::Error> {
        let name = value.get_colname();
        let Some(t) = value.get_type_name().as_inner() else {
            AnalysisError::raise_unexpected_none("columndef.type_name")?
        };
        let full_name = FullName::try_from(t)?;
        let type_ref = TypeReference::concrete_type_ref(&full_name, false);

        Ok(Self {
            name,
            field_type: Some(type_ref),
            declaration_collation: crate::sem::DeclarationCollation::default(),
            requested_type: None,
            is_not_null: Some(value.get_is_not_null()),
        })
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
