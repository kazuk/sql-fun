use super::IndirectionExpr;
use crate::sem::{
    AnalysisError, AnalysisProblem, ParseContext, SemScalarExpr, TypeDefinition, TypeReference,
};

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MemberRef {
    /// composite type
    base: SemScalarExpr,
    /// accessing member
    member: String,
    /// member type
    member_type: Option<TypeReference>,
}

impl MemberRef {
    pub fn new(base: SemScalarExpr, member: String, member_type: Option<TypeReference>) -> Self {
        Self {
            base,
            member,
            member_type,
        }
    }
    pub fn base_is_not_null(&self) -> Option<bool> {
        self.base.is_not_null()
    }

    pub fn member_type(&self) -> &Option<TypeReference> {
        &self.member_type
    }

    fn test_member_exist<TParseContext>(
        member: &str,
        context: &mut TParseContext,
        td: &TypeDefinition,
        ty: &TypeReference,
    ) -> Result<Option<TypeReference>, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        if let Some(member_type) = td.get_member_type(context, member) {
            Ok(Some(member_type))
        } else {
            context.report_problem(AnalysisProblem::composite_type_field_not_found(ty, member))?;
            Ok(None)
        }
    }

    pub fn member_reference<TParseContext>(
        source: SemScalarExpr,
        member: String,
        context: &mut TParseContext,
    ) -> Result<IndirectionExpr, AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let member_type = if let Some(ty) = &source.get_type() {
            if let Some(td) = ty.type_definition().cloned() {
                Self::test_member_exist(&member, context, &td, ty)?
            } else if let Some(td) = context.get_type(ty.full_name()).cloned() {
                Self::test_member_exist(&member, context, &td, ty)?
            } else {
                context.report_problem(AnalysisProblem::member_access_requires_composite_type(
                    ty.full_name(),
                    &member,
                ))?;
                None
            }
        } else {
            None
        };
        Ok(IndirectionExpr::MemberRef(Box::new(MemberRef::new(
            source,
            member,
            member_type,
        ))))
    }
}
