use crate::{
    ast::Expr,
    sem::{ImplicitCastExpr, TypeCastExpr, WildcardQualifiers, WildcardRewriteError},
};
impl ImplicitCastExpr {
    pub(crate) fn projection_expression(
        &self,
        bindings: &WildcardQualifiers,
    ) -> Result<Expr, WildcardRewriteError> {
        let (argument, target) = self.projection_operand();
        argument.projection_cast(target, bindings)
    }
}

impl TypeCastExpr {
    pub(crate) fn projection_expression(
        &self,
        bindings: &WildcardQualifiers,
    ) -> Result<Expr, WildcardRewriteError> {
        let (argument, target) = self.projection_operand();
        argument.projection_cast(target, bindings)
    }
}
