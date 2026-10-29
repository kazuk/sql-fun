//! Mutable semantic-tree traversal. Visitors never repeat name resolution.
use crate::sem::{AnalysisError, SemScalarExpr};

pub(crate) trait SemanticVisitor {
    fn table_binding(&mut self, _name: &mut crate::sem::TableName) {}

    fn relation_identity(&mut self, _name: &crate::sem::FullName) {}
    // Normalize context-free expressions before walking their newly analyzed children.
    fn enter_expression(&mut self, _expression: &mut SemScalarExpr) -> Result<(), AnalysisError> {
        Ok(())
    }
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
    fn array_subquery(
        &mut self,
        _value: &mut crate::sem::ArraySubqueryExpr,
    ) -> Result<(), AnalysisError> {
        Ok(())
    }
    fn membership(&mut self, _value: &mut crate::sem::MembershipExpr) -> Result<(), AnalysisError> {
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

/// Rebind exact analyzed identities while preserving SQL aliases and spelling.
pub(crate) struct RelationRenamer<'a> {
    old: &'a crate::sem::TableName,
    new: &'a crate::sem::TableName,
}
impl<'a> RelationRenamer<'a> {
    pub(crate) const fn new(
        old: &'a crate::sem::TableName,
        new: &'a crate::sem::TableName,
    ) -> Self {
        Self { old, new }
    }
}
impl SemanticVisitor for RelationRenamer<'_> {
    fn table_binding(&mut self, name: &mut crate::sem::TableName) {
        name.rename_reference(self.old, self.new);
    }
    fn expression(&mut self, expression: &mut SemScalarExpr) -> Result<(), AnalysisError> {
        expression.rename_bound_relation(self.old, self.new);
        Ok(())
    }
}
