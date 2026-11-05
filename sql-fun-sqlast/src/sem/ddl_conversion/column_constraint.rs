//! Payload-owned conversion of column constraint syntax.
use crate::sem::SemScalarExpr;
use crate::{
    ast::Expr,
    context::ParseContext,
    names::{ColumnName, TableName},
    sem::{
        AnalysisError, AnalysisProblem, AttrDeferrabilityColumnConstraint,
        AttrDeferredColumnConstraint, AttrImmediateColumnConstraint, CollateColumnConstraint,
        ColumnNullabilityConstraint, DefaultColumnConstraint, ExclusionColumnConstraint,
        GeneratedColumnConstraint, IdentityColumnConstraint, PrimaryKeyColumnConstraint,
        ScalarAnalysisContext, UniqueColumnConstraint,
    },
    syntax_options::{
        ConflictClause, Deferrable, GeneratedColumnMode, IdentityColumnMode, Validity,
    },
};

impl ColumnNullabilityConstraint {
    pub(super) fn from_ast_parts(
        _context: &mut impl ParseContext,
        table: &TableName,
        column: &ColumnName,
        not_null: bool,
        conflict: &Option<ConflictClause>,
        validity: &Option<Validity>,
    ) -> Self {
        Self::new(table, column, not_null, conflict.clone(), validity.clone())
    }
}

impl DefaultColumnConstraint {
    pub(super) fn from_ast_expression(
        context: &mut impl ParseContext,
        table: &TableName,
        column: &ColumnName,
        expression: &Expr,
    ) -> Result<Self, AnalysisError> {
        Ok(Self::new(
            table,
            column,
            SemScalarExpr::from_ast_in(expression, context, &ScalarAnalysisContext::default())?,
        ))
    }
}

impl PrimaryKeyColumnConstraint {
    pub(super) fn from_ast_parts(
        _context: &mut impl ParseContext,
        table: &TableName,
        column: &ColumnName,
        conflict: &Option<ConflictClause>,
        deferrable: &Option<Deferrable>,
    ) -> Self {
        Self::new(table, column, conflict.clone(), deferrable.clone())
    }
}

impl UniqueColumnConstraint {
    pub(super) fn from_ast_conflict(
        _context: &mut impl ParseContext,
        table: &TableName,
        column: &ColumnName,
        conflict: &Option<ConflictClause>,
    ) -> Self {
        Self::new(table, column, conflict.clone())
    }
}

impl IdentityColumnConstraint {
    pub(super) fn from_ast_mode(
        _context: &mut impl ParseContext,
        table: &TableName,
        column: &ColumnName,
        mode: &IdentityColumnMode,
    ) -> Self {
        Self::new(table, column, mode.clone())
    }
}

impl GeneratedColumnConstraint {
    pub(super) fn from_ast_expression(
        context: &mut impl ParseContext,
        table: &TableName,
        column: &ColumnName,
        expression: &Expr,
        mode: &Option<GeneratedColumnMode>,
    ) -> Result<Self, AnalysisError> {
        Ok(Self::new(
            table,
            column,
            SemScalarExpr::from_ast_in(expression, context, &ScalarAnalysisContext::default())?,
            mode.clone(),
        ))
    }
}

impl ExclusionColumnConstraint {
    pub(super) fn from_ast_declaration(
        _context: &mut impl ParseContext,
        table: &TableName,
        column: &ColumnName,
    ) -> Self {
        Self::new(table, column)
    }
}

impl AttrDeferrabilityColumnConstraint {
    pub(super) fn from_ast_value(
        _context: &mut impl ParseContext,
        table: &TableName,
        column: &ColumnName,
        value: bool,
    ) -> Self {
        Self::new(table, column, value)
    }
}

impl AttrDeferredColumnConstraint {
    pub(super) fn from_ast_declaration(
        _context: &mut impl ParseContext,
        table: &TableName,
        column: &ColumnName,
    ) -> Self {
        Self::new(table, column)
    }
}

impl AttrImmediateColumnConstraint {
    pub(super) fn from_ast_declaration(
        _context: &mut impl ParseContext,
        table: &TableName,
        column: &ColumnName,
    ) -> Self {
        Self::new(table, column)
    }
}

impl CollateColumnConstraint {
    pub(super) fn from_ast_name(
        context: &mut impl ParseContext,
        table: &TableName,
        column: &ColumnName,
        name: &str,
    ) -> Result<Self, AnalysisError> {
        if !Self::is_known_name(name) {
            context.report_analysis_problem(AnalysisProblem::collation_name_not_found(name))?;
        }
        Ok(Self::new(table, column, name))
    }
}
