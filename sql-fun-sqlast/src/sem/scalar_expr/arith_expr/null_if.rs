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
    pub fn build_null_if<TParseContext>(
        context: TParseContext,
        lexpr: Option<SemScalarExpr>,
        rexpr: Option<SemScalarExpr>,
    ) -> Result<(SemScalarExpr, TParseContext), AnalysisError> {
        let Some(lexpr) = lexpr else {
            AnalysisError::raise_unexpected_none("nullif.lexpr")?
        };
        let Some(rexpr) = rexpr else {
            AnalysisError::raise_unexpected_none("nullif.rexpr")?
        };

        let result = Self::NullIfExpr(Box::new(NullIfExpr {
            return_value: lexpr,
            comparison_value: rexpr,
        }));
        Ok((SemScalarExpr::Arith(result), context))
    }
}

impl NullIfExpr {
    pub(crate) fn walk<V: crate::sem::traversal::SemanticVisitorMut>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        self.return_value.walk(visitor)?;
        self.comparison_value.walk(visitor)
    }
    pub(crate) fn visit<V: crate::sem::traversal::SemanticVisitor>(
        &self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        self.return_value.visit(visitor)?;
        self.comparison_value.visit(visitor)
    }
}

impl NullIfExpr {
    pub(crate) fn collect_constraints(
        &self,
        parameters: &mut crate::sem::parameter_resolution::ParameterResolution<'_>,
    ) {
        if self.return_value.constraint_node().is_some()
            || self.comparison_value.constraint_node().is_some()
        {
            parameters.special_operator("NULLIF", &[&self.return_value, &self.comparison_value]);
        }
    }
}
