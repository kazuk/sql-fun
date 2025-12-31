use crate::sem::{AnalysisError, SemScalarExpr, TypeReference, scalar_expr::SemScalarExprNode};

use super::ArithExpr;

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NullIfExpr {
    return_value: SemScalarExpr,
    comparison_value: SemScalarExpr,
}

impl NullIfExpr {
    pub fn is_not_null(&self) -> Option<bool> {
        Some(false)
    }

    pub fn is_strict(&self) -> bool {
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
