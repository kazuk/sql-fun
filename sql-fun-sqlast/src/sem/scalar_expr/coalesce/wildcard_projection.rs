use crate::{
    ast::{Call, Expr},
    sem::{CoalesceExpr, WildcardQualifiers, WildcardRewriteError},
};
impl CoalesceExpr {
    pub(crate) fn projection_expression(
        &self,
        bindings: &WildcardQualifiers,
    ) -> Result<Expr, WildcardRewriteError> {
        let mut arguments = Vec::new();
        for argument in self.arguments() {
            arguments.push(argument.projection_expression(bindings)?);
        }
        Ok(Expr::projection_call(&Call::coalesce_projection(
            &arguments,
        )))
    }
}
