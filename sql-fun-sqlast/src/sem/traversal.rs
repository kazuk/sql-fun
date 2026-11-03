//! Shared-reference and mutable semantic-tree traversal without name resolution.
use crate::sem::{AnalysisError, SemScalarExpr};

/// Observe completed semantic bindings through shared references only.
/// Visitor state can change; semantic nodes and their retained metadata cannot.
pub(crate) trait SemanticVisitor {
    fn enter_rows_functions(&mut self) {}
    fn leave_rows_functions(&mut self, _alias: &crate::sem::AliasSpec, _ordinality: bool) {}
    fn access_purpose(&mut self, _kind: crate::sem::AccessKind) {}
    fn access_join(&mut self, _join: &crate::sem::data_source::JoinExprDataSource) {}
    fn leave_access_join(&mut self) {}
    fn access_action(&mut self, _name: &crate::names::FullName, _kind: crate::sem::AccessKind) {}
    fn function_binding(
        &mut self,
        _name: &crate::names::FullName,
        _alias: &crate::sem::AliasSpec,
        _columns: &[crate::names::ColumnName],
    ) {
    }

    // Placement validators inspect written operands, not definitions behind bindings.
    fn walk_source_column_values(&self) -> bool {
        true
    }
    fn enter_join(&mut self) {}
    fn join_right(&mut self) {}
    fn leave_join(
        &mut self,
        _using: &[crate::names::ColumnName],
        _natural: bool,
        _alias: Option<&crate::sem::AliasSpec>,
    ) {
    }
    fn source_outputs(&mut self, _columns: &[crate::names::ColumnName]) {}
    fn assignment_target(&mut self, _target: &SemScalarExpr) {}
    fn enter_cte(&mut self, _name: &str, _columns: &[crate::names::ColumnName]) {}
    fn cte_columns(&mut self, _name: &str, _columns: &[crate::names::ColumnName]) {}
    fn cte_source(&mut self, _name: &str, _alias: &crate::sem::AliasSpec) {}
    fn table_binding(&mut self, _name: &crate::names::TableName) {}

    fn relation_identity(&mut self, _name: &crate::names::FullName) {}
    // Observe the node before its already-analyzed children.
    fn enter_expression(&mut self, _expression: &SemScalarExpr) -> Result<(), AnalysisError> {
        Ok(())
    }
    fn expression(&mut self, _expression: &SemScalarExpr) -> Result<(), AnalysisError> {
        Ok(())
    }
    fn sampling_input(&mut self, _value: &SemScalarExpr, _target: &crate::sem::TypeReference) {}
    fn table_sample(
        &mut self,
        _sample: &crate::sem::TableSampleSource,
    ) -> Result<(), AnalysisError> {
        Ok(())
    }
    fn assignment(
        &mut self,
        _value: &SemScalarExpr,
        _target: &crate::sem::InsertTarget,
    ) -> Result<(), AnalysisError> {
        Ok(())
    }
    fn assignment_value(
        &mut self,
        _value: &SemScalarExpr,
        _target: &crate::sem::TypeReference,
    ) -> Result<(), AnalysisError> {
        Ok(())
    }
    fn dml_returning(
        &mut self,
        columns: &crate::sem::ResultColumnCollection,
    ) -> Result<(), AnalysisError>
    where
        Self: Sized,
    {
        columns.visit(self)
    }
    fn array_subquery(
        &mut self,
        _value: &crate::sem::ArraySubqueryExpr,
    ) -> Result<(), AnalysisError> {
        Ok(())
    }
    fn membership(&mut self, _value: &crate::sem::MembershipExpr) -> Result<(), AnalysisError> {
        Ok(())
    }
    fn subscript(&mut self, _value: &SemScalarExpr) {}
    // UPDATE/DELETE finalize subquery projections before surrounding uses.
    // INSERT SELECT instead receives destination assignment context.
    fn enter_dml(&mut self) {}
    fn leave_dml(&mut self) {}
    fn enter_query(&mut self) {}
    fn enter_set_operation(&mut self) {}
    fn leave_set_operation(&mut self) {}
    fn set_output(&mut self, _index: usize, name: &str, value: &SemScalarExpr) {
        self.output(name, value);
    }
    fn enter_set_operand(&mut self) {}
    fn leave_set_operand(&mut self) {}
    fn leave_query(&mut self) {}
    fn cte(&mut self, _name: &str) {}
    fn relation(&mut self, _name: Option<&str>, _alias: &str, _columns: &[String]) {}
    fn derived(&mut self, _alias: &str, _columns: &[String]) {}
    fn output(&mut self, _name: &str, _value: &SemScalarExpr) {}
    /// Every VALUES row contributes to its output positions.
    fn values_output(&mut self, _index: usize, name: &str, value: &SemScalarExpr) {
        self.output(name, value);
    }
    /// Plain DISTINCT compares all completed SELECT output positions.
    fn distinct_outputs(&mut self) {}
    fn join_sensitive_star(&mut self) {}
    fn projection(
        &mut self,
        _name: &Option<crate::names::ColumnName>,
        _explicit: bool,
        _expanded: bool,
        _value: &SemScalarExpr,
    ) -> Result<(), AnalysisError> {
        Ok(())
    }
    fn result_name(
        &mut self,
        _name: &Option<crate::names::ColumnName>,
        _explicit: bool,
        _value: &SemScalarExpr,
    ) {
    }
    fn bound_definition(
        &mut self,
        _column: &crate::sem::ColumnDefinition,
        _destination: &SemScalarExpr,
    ) {
    }
    fn derived_binding(
        &mut self,
        _alias: &crate::sem::AliasSpec,
        _columns: &[crate::names::ColumnName],
    ) {
    }
    fn source_binding(
        &mut self,
        _relation: &crate::names::FullName,
        _alias: &crate::sem::AliasSpec,
    ) {
    }

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
    fn set_operation(
        &mut self,
        _query: &crate::sem::SetOperationQuery,
    ) -> Result<(), AnalysisError> {
        Ok(())
    }
}

/// Rewrite semantic nodes and refresh metadata through mutable references.
pub(crate) trait SemanticVisitorMut {
    // Placement validators inspect written operands, not definitions behind bindings.
    fn walk_source_column_values(&self) -> bool {
        true
    }
    fn enter_join(&mut self) {}
    fn join_right(&mut self) {}
    fn leave_join(
        &mut self,
        _using: &[crate::names::ColumnName],
        _natural: bool,
        _alias: Option<&crate::sem::AliasSpec>,
    ) {
    }
    fn source_outputs(&mut self, _columns: &[crate::names::ColumnName]) {}
    fn assignment_target(&mut self, _target: &mut SemScalarExpr) {}
    fn cte_columns(&mut self, _name: &str, _columns: &[crate::names::ColumnName]) {}
    fn cte_source(&mut self, _name: &str, _alias: &crate::sem::AliasSpec) {}
    fn table_binding(&mut self, _name: &mut crate::names::TableName) {}

    fn relation_identity(&mut self, _name: &crate::names::FullName) {}
    // Normalize context-free expressions before walking their newly analyzed children.
    fn enter_expression(&mut self, _expression: &mut SemScalarExpr) -> Result<(), AnalysisError> {
        Ok(())
    }
    fn expression(&mut self, _expression: &mut SemScalarExpr) -> Result<(), AnalysisError> {
        Ok(())
    }
    fn sampling_input(&mut self, _value: &SemScalarExpr, _target: &crate::sem::TypeReference) {}
    fn table_sample(
        &mut self,
        _sample: &mut crate::sem::TableSampleSource,
    ) -> Result<(), AnalysisError> {
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
    fn enter_set_operand(&mut self) {}
    fn leave_set_operand(&mut self) {}
    fn leave_query(&mut self) {}
    fn cte(&mut self, _name: &str) {}
    fn relation(&mut self, _name: Option<&str>, _alias: &str, _columns: &[String]) {}
    fn derived(&mut self, _alias: &str, _columns: &[String]) {}
    fn output(&mut self, _name: &str, _value: &mut SemScalarExpr) {}
    fn join_sensitive_star(&mut self) {}
    fn projection(
        &mut self,
        _name: &mut Option<crate::names::ColumnName>,
        _explicit: bool,
        _expanded: bool,
        _value: &mut SemScalarExpr,
    ) -> Result<(), AnalysisError> {
        Ok(())
    }
    fn result_name(
        &mut self,
        _name: &mut Option<crate::names::ColumnName>,
        _explicit: bool,
        _value: &SemScalarExpr,
    ) {
    }
    fn bound_definition(
        &mut self,
        _column: &mut crate::sem::ColumnDefinition,
        _destination: &SemScalarExpr,
    ) {
    }
    fn derived_binding(
        &mut self,
        _alias: &crate::sem::AliasSpec,
        _columns: &[crate::names::ColumnName],
    ) {
    }
    fn source_binding(
        &mut self,
        _relation: &crate::names::FullName,
        _alias: &crate::sem::AliasSpec,
    ) {
    }

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
    fn set_operation(
        &mut self,
        _query: &mut crate::sem::SetOperationQuery,
    ) -> Result<(), AnalysisError> {
        Ok(())
    }
}

pub(crate) fn optional<V: SemanticVisitorMut>(
    value: &mut Option<SemScalarExpr>,
    visitor: &mut V,
) -> Result<(), AnalysisError> {
    if let Some(value) = value {
        value.walk(visitor)?;
    }
    Ok(())
}

pub(crate) fn visit_optional<V: SemanticVisitor>(
    value: &Option<SemScalarExpr>,
    visitor: &mut V,
) -> Result<(), AnalysisError> {
    if let Some(value) = value {
        value.visit(visitor)?;
    }
    Ok(())
}

/// Rebind exact analyzed identities while preserving SQL aliases and spelling.
pub(crate) struct RelationRenamer<'a> {
    old: &'a crate::names::TableName,
    new: &'a crate::names::TableName,
}
impl<'a> RelationRenamer<'a> {
    pub(crate) const fn new(
        old: &'a crate::names::TableName,
        new: &'a crate::names::TableName,
    ) -> Self {
        Self { old, new }
    }
}
impl SemanticVisitorMut for RelationRenamer<'_> {
    fn table_binding(&mut self, name: &mut crate::names::TableName) {
        name.rename_reference(self.old, self.new);
    }
    fn expression(&mut self, expression: &mut SemScalarExpr) -> Result<(), AnalysisError> {
        expression.rename_bound_relation(self.old, self.new);
        Ok(())
    }
}

/// Changes bound source columns without changing independently named query outputs.
pub(crate) struct ColumnRenamer<'a> {
    rename: &'a crate::sem::ColumnRename,
    assignment_changed: bool,
}
impl<'a> ColumnRenamer<'a> {
    pub(crate) const fn new(rename: &'a crate::sem::ColumnRename) -> Self {
        Self {
            rename,
            assignment_changed: false,
        }
    }
}
impl SemanticVisitorMut for ColumnRenamer<'_> {
    fn assignment_target(&mut self, target: &mut SemScalarExpr) {
        self.assignment_changed |= target.matches_column(self.rename);
        target.rename_bound_column(self.rename);
    }
    fn bound_definition(
        &mut self,
        column: &mut crate::sem::ColumnDefinition,
        _destination: &SemScalarExpr,
    ) {
        if std::mem::take(&mut self.assignment_changed) {
            column.rename_column_name(self.rename);
        }
    }

    fn result_name(
        &mut self,
        name: &mut Option<crate::names::ColumnName>,
        explicit: bool,
        value: &SemScalarExpr,
    ) {
        if self.rename.rewrites_outputs()
            && !explicit
            && let Some(output) = value.renamed_column_output(self.rename)
        {
            *name = Some(output.clone());
        }
    }

    fn expression(&mut self, expression: &mut SemScalarExpr) -> Result<(), AnalysisError> {
        expression.rename_bound_column(self.rename);
        Ok(())
    }
}
