use crate::{
    sem::{AstAndContextPair, ColumnDefinition, ColumnName, Nullable},
    syn::{ColumnDef, ListOpt, Opt},
};

use super::{AnalysisError, ColumnCollection, FullName, ParseContext, SemAst, TypeReference};

/// collection for composite type field
#[derive(Debug, Default, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CompositeFieldCollection(Vec<CompositeFieldDefinition>);

impl CompositeFieldCollection {
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
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CreateCompositType {
    type_ref: Option<TypeReference>,
    fields: CompositeFieldCollection,
}

impl CreateCompositType {
    /// name of composite type,
    ///
    /// When instanse is dynamic type and no-name , returns None
    #[must_use]
    pub fn name(&self) -> Option<&FullName> {
        self.type_ref.as_ref().map(|f| f.full_name())
    }

    /// get concrete type reference
    ///
    /// When instance is dynamic type and no-named returns Name
    #[must_use]
    pub fn type_ref(&self) -> &Option<TypeReference> {
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
        let Some(typevar) = syn.get_typevar().as_inner() else {
            AnalysisError::raise_unexpected_none("composite_type_stmt.typevar")?
        };
        let Some(coldeflist) = syn.get_coldeflist().map(|v| v.as_column_def()) else {
            AnalysisError::raise_unexpected_none("composite_type.coldeflist")?
        };
        let mut fields = CompositeFieldCollection::default();
        for coldef in coldeflist {
            fields.push(CompositeFieldDefinition::try_from(&coldef)?);
        }

        let name = FullName::try_from(typevar)?;
        let comp_def = CreateCompositType {
            type_ref: Some(TypeReference::from_full_name(name.clone())),
            fields,
        };
        let result_context = context.apply_create_comp_type(&comp_def)?;

        Ok(AstAndContextPair::new(
            SemAst::CreateCompType(comp_def),
            result_context,
        ))
    }
}

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CompositeFieldDefinition {
    name: String,
    field_type: Option<TypeReference>,
    is_not_null: Option<Nullable>,
}

impl CompositeFieldDefinition {
    pub fn new(
        name: &str,
        field_type: &Option<TypeReference>,
        is_not_null: &Option<Nullable>,
    ) -> Self {
        Self {
            name: name.to_string(),
            field_type: field_type.clone(),
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
            is_not_null: Some(value.get_is_not_null()),
        })
    }
}

/// analyze [`crate::syn::CompositeTypeStmt`]
#[deprecated = "use CreateCompositType::analyze_create_comp_type"]
pub fn analyze_create_comp_type<TParseContext>(
    context: TParseContext,
    parent_schema: &Option<String>,
    syn: crate::syn::CompositeTypeStmt,
) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
where
    TParseContext: ParseContext,
{
    CreateCompositType::analyze_create_comp_type(context, parent_schema, syn)
}
