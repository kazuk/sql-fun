use crate::sem::scalar_expr::indirect::IndirectionExpr;
use crate::sem::{AnalysisError, AnalysisProblem, ParseContext, SemScalarExpr, TypeReference};

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MemberRef {
    /// composite type
    base: SemScalarExpr,
    /// accessing member
    member: String,
    /// member type
    member_type: Option<TypeReference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    member_not_null: Option<bool>,
}

impl MemberRef {
    pub(super) fn rename_column(&mut self, rename: &crate::sem::ColumnRename) {
        if self.base.get_type().is_some_and(|ty| {
            ty == TypeReference::concrete_type_ref(rename.table().full_name(), false)
        }) && self
            .base
            .as_whole_row()
            .is_none_or(|row| row.has_renamed_field(rename))
        {
            let mut name = crate::sem::ColumnName::from(self.member.as_str());
            rename.rename_name(&mut name);
            self.member = name.as_str().to_owned();
        }
    }

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

    pub(super) fn row_projection_name(&self) -> &str {
        if self.base.row_constructor().is_some()
            || self.base.constructor_projection_name() == Some("row")
        {
            &self.member
        } else {
            ""
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
        source: &SemScalarExpr,
        ty: &TypeReference,
    ) -> Result<(Option<TypeReference>, Option<bool>), AnalysisError>
    where
        TParseContext: crate::sem::ParseContext,
    {
        if let Some(columns) = source.composite_columns(context)
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
            if ty.definition_from_catalog(context).is_some() {
                Self::test_member_exist(&member, context, &source, ty)?
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
    pub(super) fn row_constructor(&self) -> Option<&crate::sem::scalar_expr::RowExpr> {
        self.base
            .row_constructor()?
            .field(&self.member)?
            .row_constructor()
    }

    pub(super) fn set_member_type(&mut self, ty: &Option<TypeReference>) {
        self.member_type.clone_from(ty);
    }

    pub(super) fn collect_constraints(
        &self,
        parameters: &mut crate::sem::parameter_resolution::ParameterResolution,
    ) -> Option<SemScalarExpr> {
        if let Some(row) = self.base.row_constructor()
            && let Some(field) = row.field(&self.member)
            && field.constraint_node().is_some()
        {
            let mut result = parameters.row_field(field);
            result.retain_indirection(&IndirectionExpr::MemberRef(Box::new(self.clone())));
            return Some(result);
        }
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
