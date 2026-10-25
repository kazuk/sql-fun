use crate::sem::{AnalysisError, SemScalarExpr, TypeReference, scalar_expr::SemScalarExprNode};

use super::ArithExpr;

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NullIfExpr {
    return_value: SemScalarExpr,
    comparison_value: SemScalarExpr,
}

impl NullIfExpr {
    pub const fn is_not_null(&self) -> Option<bool> {
        Some(false)
    }

    pub const fn is_strict(&self) -> bool {
        false
    }
}

impl SemScalarExprNode for NullIfExpr {
    fn get_type(&self) -> Option<TypeReference> {
        self.return_value.get_type()
    }

    fn is_not_null(&self) -> Option<bool> {
        Some(false)
    }
}

impl ArithExpr {
    /// create instance
    pub fn build_null_if<TParseContext: crate::sem::ParseContext>(
        mut context: TParseContext,
        lexpr: Option<SemScalarExpr>,
        rexpr: Option<SemScalarExpr>,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError> {
        let value = Self::build_null_if_resolved(&mut context, lexpr, rexpr)?;
        Ok((value, context))
    }

    pub(crate) fn build_null_if_resolved(
        context: &mut impl crate::sem::ParseContext,
        lexpr: Option<SemScalarExpr>,
        rexpr: Option<SemScalarExpr>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let Some(lexpr) = lexpr else {
            AnalysisError::raise_unexpected_none("nullif.lexpr")?
        };
        let Some(rexpr) = rexpr else {
            AnalysisError::raise_unexpected_none("nullif.rexpr")?
        };

        let binding = crate::sem::ResolvedOperator::comparison(
            context,
            &crate::sem::OperatorName::from_parts(None, "="),
        );
        let expression = crate::sem::SpecialOperatorExpr::resolve(
            context,
            crate::sem::SpecialOperatorKind::null_if(),
            &[lexpr, rexpr],
            None,
            &[binding],
        )?;
        Ok(SemScalarExpr::special_operator(&expression))
    }
}

impl NullIfExpr {
    pub(crate) fn walk<V: crate::sem::traversal::SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        self.return_value.walk(visitor)?;
        self.comparison_value.walk(visitor)
    }
}

impl NullIfExpr {
    pub(crate) fn collect_constraints(
        &self,
        parameters: &mut crate::sem::parameter_resolution::ParameterResolution,
        context: &mut impl crate::sem::ParseContext,
    ) -> Result<Option<SemScalarExpr>, AnalysisError> {
        let binding = crate::sem::ResolvedOperator::comparison(
            context,
            &crate::sem::OperatorName::from_parts(None, "="),
        );
        let mut expression = crate::sem::SpecialOperatorExpr::resolve(
            context,
            crate::sem::SpecialOperatorKind::null_if(),
            &[self.return_value.clone(), self.comparison_value.clone()],
            None,
            &[binding],
        )?;
        expression.collect_constraints(parameters, context)?;
        Ok(Some(SemScalarExpr::special_operator(&expression)))
    }
}
