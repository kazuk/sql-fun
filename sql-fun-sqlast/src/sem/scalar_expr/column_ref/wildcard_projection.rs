use crate::{
    ast::{Expr, Name},
    sem::{ColumnReferenceExpr, WildcardQualifiers, WildcardRewriteError},
};
impl ColumnReferenceExpr {
    pub(crate) fn projection_expression(
        &self,
        bindings: &WildcardQualifiers,
    ) -> Result<Expr, WildcardRewriteError> {
        if let Some(table) = self.table_column_reference()
            && let Some(qualifier) = bindings.reference_for(table.relation(), table.alias())
        {
            let mut parts = qualifier.to_vec();
            parts.push(self.get_column_name().to_string());
            return Ok(Expr::projection_column(&Name::from_parts(&parts)));
        }
        let Some((alias, column)) = self.projection_reference() else {
            return Err(WildcardRewriteError::invalid(
                "wildcard column binding is unresolved",
            ));
        };
        let parts = if alias.is_empty() {
            vec![column.to_string()]
        } else {
            vec![alias.to_owned(), column.to_string()]
        };
        Ok(Expr::projection_column(&Name::from_parts(&parts)))
    }
}
