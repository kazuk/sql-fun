use crate::sem::{SemScalarExpr, TypeReference, scalar_expr::SemScalarExprNode};

/// array expression
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ArrayExpr(Vec<SemScalarExpr>);

impl ArrayExpr {
    pub(super) fn from_values(values: &[SemScalarExpr]) -> Self {
        Self(values.to_vec())
    }
}

impl SemScalarExprNode for ArrayExpr {
    fn get_type(&self) -> Option<crate::sem::TypeReference> {
        let elm_type = self.0.first()?.get_type()?;
        Some(TypeReference::concrete_type_ref(elm_type.full_name(), true))
    }

    fn is_not_null(&self) -> Option<bool> {
        todo!()
    }
}

impl ArrayExpr {
    pub(crate) fn walk<V: crate::sem::traversal::SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), crate::sem::AnalysisError> {
        for value in &mut self.0 {
            value.walk(visitor)?;
        }
        Ok(())
    }
}
