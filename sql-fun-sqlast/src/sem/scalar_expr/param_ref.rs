use crate::sem::TypeReference;
use crate::sem::scalar_expr::SemScalarExprNode;

/// parameter reference expression
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ParamRef {}

impl SemScalarExprNode for ParamRef {
    fn get_type(&self) -> Option<TypeReference> {
        todo!()
    }

    fn is_not_null(&self) -> Option<bool> {
        todo!()
    }
}
