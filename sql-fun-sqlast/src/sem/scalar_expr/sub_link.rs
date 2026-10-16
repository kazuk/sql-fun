use crate::sem::scalar_expr::SemScalarExprNode;
use crate::sem::{AnalysisError, SelectStatement, TypeReference};

/// Sub-query expression
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SubLinkExpr {
    subselect: Box<SelectStatement>,
    type_ref: TypeReference,
}

impl SubLinkExpr {
    pub(super) fn from_resolved(query: &SelectStatement) -> Result<Self, AnalysisError> {
        Ok(Self {
            subselect: Box::new(query.clone()),
            type_ref: query.returning_composite_type()?,
        })
    }
}

impl SemScalarExprNode for SubLinkExpr {
    fn get_type(&self) -> Option<TypeReference> {
        self.subselect.result_columns().scalar_type()
    }

    fn is_not_null(&self) -> Option<bool> {
        Some(false)
    }
}

impl SubLinkExpr {
    pub(crate) fn walk<V: crate::sem::traversal::SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), crate::sem::AnalysisError> {
        self.subselect.walk(visitor)?;
        self.type_ref = self.subselect.returning_composite_type()?;
        Ok(())
    }
}
