use crate::sem::scalar_expr::indirect::IndirectionExpr;
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
    #[serde(skip)]
    member_not_null: Option<bool>,
}

impl MemberRef {
    pub const fn new(
        base: SemScalarExpr,
        member: String,
        member_type: Option<TypeReference>,
        member_not_null: Option<bool>,
    ) -> Self {
        Self {
            base,
            member,
            member_type,
            member_not_null,
        }
    }
    pub fn base_is_not_null(&self) -> Option<bool> {
        match (self.base.is_not_null(), self.member_not_null) {
            (Some(false), _) | (_, Some(false)) => Some(false),
            (Some(true), Some(true)) => Some(true),
            _ => None,
        }
    }

    /// Base expression whose composite field is accessed.
    pub const fn base(&self) -> &SemScalarExpr {
        &self.base
    }

    /// Field identifier after SQL identifier normalization.
    pub fn member(&self) -> &str {
        &self.member
    }

    pub const fn member_type(&self) -> &Option<TypeReference> {
        &self.member_type
    }

    fn test_member_exist<TParseContext>(
        member: &str,
        context: &mut TParseContext,
        td: &TypeDefinition,
        ty: &TypeReference,
    ) -> Result<(Option<TypeReference>, Option<bool>), AnalysisError>
    where
        TParseContext: crate::sem::ParseContext,
    {
        if let Some(columns) = td.row_columns(context)
            && let Some(field) = columns.get_column_def(&crate::sem::ColumnName::from(member))
        {
            Ok((field.get_type_cloned(), field.is_not_null()))
        } else {
            context.report_analysis_problem(AnalysisProblem::composite_type_field_not_found(
                ty, member,
            ))?;
            Ok((None, None))
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
        Self::member_resolved(source, member, context)
    }

    pub(super) fn member_resolved<TContext: crate::sem::ParseContext>(
        source: SemScalarExpr,
        member: String,
        context: &mut TContext,
    ) -> Result<IndirectionExpr, AnalysisError> {
        let (member_type, member_not_null) = if let Some(ty) = &source.get_type() {
            if let Some(td) = ty.type_definition().cloned() {
                Self::test_member_exist(&member, context, &td, ty)?
            } else if let Some(td) = context.get_type(ty.full_name()).cloned() {
                Self::test_member_exist(&member, context, &td, ty)?
            } else {
                context.report_analysis_problem(
                    AnalysisProblem::member_access_requires_composite_type(ty.full_name(), &member),
                )?;
                (None, None)
            }
        } else {
            (None, None)
        };
        Ok(IndirectionExpr::MemberRef(Box::new(Self::new(
            source,
            member,
            member_type,
            member_not_null,
        ))))
    }
}

impl MemberRef {
    pub(crate) fn walk<V: crate::sem::traversal::SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), crate::sem::AnalysisError> {
        self.base.walk(visitor)?;
        Ok(())
    }
}

impl MemberRef {
    pub(super) fn set_member_type(&mut self, ty: &Option<TypeReference>) {
        self.member_type.clone_from(ty);
    }

    pub(super) fn collect_constraints(
        &self,
        parameters: &mut crate::sem::parameter_resolution::ParameterResolution,
    ) -> Option<SemScalarExpr> {
        if self.base.constraint_node().is_none() && !self.base.needs_type_inference() {
            return None;
        }
        let mut result = parameters.member_access(&self.base, &self.member);
        result.retain_indirection(&IndirectionExpr::MemberRef(Box::new(self.clone())));
        Some(result)
    }
}

impl MemberRef {
    pub(super) fn walk_assignment_target<V: crate::sem::traversal::SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        self.base.walk_assignment_target(visitor)?;
        Ok(())
    }
}
