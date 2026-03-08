use crate::{sem::AstAndContextPair, syn::ListOpt};

use super::{AnalysisError, FullName, ParseContext, SemAst};

/// analyzed [`crate::syn::CreateEnumStmt`]
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CreateEnum {
    name: FullName,
    values: Vec<EnumValueDefinition>,
}

impl CreateEnum {
    /// get name
    #[must_use]
    pub fn name(&self) -> &FullName {
        &self.name
    }

    /// get values
    #[must_use]
    pub fn values(&self) -> &Vec<EnumValueDefinition> {
        &self.values
    }

    /// analyze [`crate::syn::CreateEnumStmt`]
    pub fn analyze_create_enum_type<TParseContext>(
        context: TParseContext,
        _parent_schema: &Option<String>,
        create: crate::syn::CreateEnumStmt,
    ) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let name = FullName::try_from(create.get_type_name())?;
        let Some(enum_values) = create.get_vals().map(|v| v.as_string()) else {
            AnalysisError::raise_unexpected_none("create_enum_stmt.vals")?
        };
        let enum_values = enum_values
            .iter()
            .map(|v| EnumValueDefinition { name: v.get_sval() })
            .collect();

        let enum_def = CreateEnum {
            name,
            values: enum_values,
        };
        let result_context = context.apply_create_enum(&enum_def)?;
        Ok(AstAndContextPair::new(
            SemAst::CreateEnum(enum_def),
            result_context,
        ))
    }
}

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EnumValueDefinition {
    name: String,
}

/// analyze [`crate::syn::CreateEnumStmt`]
#[deprecated = "use CreateEnum::analyze_create_enum_type"]
pub fn analyze_create_enum_type<TParseContext>(
    context: TParseContext,
    parent_schema: &Option<String>,
    create: crate::syn::CreateEnumStmt,
) -> Result<AstAndContextPair<TParseContext>, AnalysisError>
where
    TParseContext: ParseContext,
{
    CreateEnum::analyze_create_enum_type(context, parent_schema, create)
}
