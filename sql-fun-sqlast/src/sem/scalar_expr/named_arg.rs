use crate::sem::TypeReference;
use crate::sem::scalar_expr::SemScalarExprNode;

/// named argument expression
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NamedArgExpr {}

impl SemScalarExprNode for NamedArgExpr {
    fn get_type(&self) -> Option<TypeReference> {
        todo!()
    }

    fn is_not_null(&self) -> Option<bool> {
        todo!()
    }
}
