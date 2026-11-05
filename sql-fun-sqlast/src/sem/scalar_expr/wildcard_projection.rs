use crate::{
    ast::{Expr, Name, TypeName},
    sem::{SemScalarExpr, TypeReference, WildcardQualifiers, WildcardRewriteError},
    stack::with_stack,
};
impl SemScalarExpr {
    pub(crate) fn projection_expression(
        &self,
        bindings: &WildcardQualifiers,
    ) -> Result<Expr, WildcardRewriteError> {
        with_stack(|| match self {
            Self::ColumnRef(value) => value.projection_expression(bindings),
            Self::Coalesce(value) => value.projection_expression(bindings),
            Self::ImplicitCast(value) => value.projection_expression(bindings),
            Self::TypeCast(value) => value.projection_expression(bindings),
            _ => Err(WildcardRewriteError::invalid(
                "resolved wildcard expression has no executable projection representation",
            )),
        })
    }
    pub(crate) fn projection_cast(
        &self,
        target: &TypeReference,
        bindings: &WildcardQualifiers,
    ) -> Result<Expr, WildcardRewriteError> {
        if target.type_definition().is_some() {
            return Err(WildcardRewriteError::invalid(
                "wildcard cast target has no SQL type name",
            ));
        }
        let expression = self.projection_expression(bindings)?;
        let target = TypeName::projection(
            &Name::from_full_name(
                target
                    .full_name()
                    .expect("owned cast targets rejected above"),
            ),
            target.is_array() == Some(true),
        );
        Ok(Expr::projection_cast(&expression, &target))
    }
}
