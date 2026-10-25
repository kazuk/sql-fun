//! Mutable semantic-tree traversal. Visitors never repeat name resolution.
use crate::sem::{AnalysisError, SemScalarExpr};

pub(crate) trait SemanticVisitor {
    fn relation_identity(&mut self, _name: &crate::sem::FullName) {}
    fn expression(&mut self, _expression: &mut SemScalarExpr) -> Result<(), AnalysisError> {
        Ok(())
    }
    fn assignment(
        &mut self,
        _value: &mut SemScalarExpr,
        _target: &crate::sem::InsertTarget,
    ) -> Result<(), AnalysisError> {
        Ok(())
    }
    fn assignment_value(
        &mut self,
        _value: &mut SemScalarExpr,
        _target: &crate::sem::TypeReference,
    ) -> Result<(), AnalysisError> {
        Ok(())
    }
    fn dml_returning(
        &mut self,
        columns: &mut crate::sem::ResultColumnCollection,
    ) -> Result<(), AnalysisError>
    where
        Self: Sized,
    {
        columns.walk(self)
    }
    fn special_operator(
        &mut self,
        _value: &mut crate::sem::scalar_expr::SpecialOperatorExpr,
    ) -> Result<(), AnalysisError> {
        Ok(())
    }
    fn array_subquery(
        &mut self,
        _value: &mut crate::sem::ArraySubqueryExpr,
    ) -> Result<(), AnalysisError> {
        Ok(())
    }
    fn subscript(&mut self, _value: &SemScalarExpr) {}
    // UPDATE/DELETE finalize subquery projections before surrounding uses.
    // INSERT SELECT instead receives destination assignment context.
    fn enter_dml(&mut self) {}
    fn leave_dml(&mut self) {}
    fn enter_query(&mut self) {}
    fn leave_query(&mut self) {}
    fn cte(&mut self, _name: &str) {}
    fn relation(&mut self, _name: Option<&str>, _alias: &str, _columns: &[String]) {}
    fn derived(&mut self, _alias: &str, _columns: &[String]) {}
    fn output(&mut self, _name: &str, _value: &mut SemScalarExpr) {}
    fn predicate(&mut self, _value: &SemScalarExpr) {}
    fn order_operator(
        &mut self,
        _value: &SemScalarExpr,
        _operator: &crate::sem::ResolvedOperator,
    ) -> Result<(), AnalysisError> {
        Ok(())
    }
    fn row_count(&mut self, _value: &SemScalarExpr) {}
    fn window_definition(&mut self, _window: &crate::sem::QueryWindow) {}
    fn frame_offset(
        &mut self,
        _value: &SemScalarExpr,
        _mode: &str,
        _order: &[crate::sem::QueryOrder],
        _reference: Option<&crate::sem::QueryName>,
    ) {
    }
    fn retain_set_outputs(&mut self) {}
}

pub(crate) fn optional<V: SemanticVisitor>(
    value: &mut Option<SemScalarExpr>,
    visitor: &mut V,
) -> Result<(), AnalysisError> {
    if let Some(value) = value {
        value.walk(visitor)?;
    }
    Ok(())
}
