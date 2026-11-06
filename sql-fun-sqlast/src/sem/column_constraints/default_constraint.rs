use crate::{
    ast::Expr as AstExpr,
    names::{ColumnName, TableName},
    sem::{
        AnalysisError, ColumnConstraint, ColumnConstraintKind, ColumnConstraintTrait, ColumnRename,
        ParseContext, ScalarAnalysisContext, SemScalarExpr, traversal::SemanticVisitorMut,
    },
};

/// default constraint
#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub struct DefaultColumnConstraint {
    table_name: TableName,
    column_name: ColumnName,
    expr: SemScalarExpr,
}

impl DefaultColumnConstraint {
    pub(crate) fn rename_column(&mut self, rename: &ColumnRename, owned: bool) {
        if owned {
            rename.rename_name(&mut self.column_name);
        }
    }

    pub(super) fn rename_expression(
        &mut self,
        visitor: &mut impl SemanticVisitorMut,
    ) -> Result<(), AnalysisError> {
        self.expr.walk(visitor)
    }

    pub(crate) fn rename_relation(&mut self, old: &TableName, new: &TableName) {
        self.table_name.rename_reference(old, new);
    }

    /// get default value expression
    pub const fn expr(&self) -> &SemScalarExpr {
        &self.expr
    }
}

impl DefaultColumnConstraint {
    /// create instance
    pub fn new(table_name: &TableName, column_name: &ColumnName, expr: SemScalarExpr) -> Self {
        Self {
            table_name: table_name.clone(),
            column_name: column_name.clone(),
            expr,
        }
    }
}

impl ColumnConstraintTrait for DefaultColumnConstraint {
    fn kind(&self) -> ColumnConstraintKind {
        ColumnConstraintKind::Default
    }

    fn table_name(&self) -> &TableName {
        &self.table_name
    }

    fn column_name(&self) -> &ColumnName {
        &self.column_name
    }
}

impl ColumnConstraint {
    /// create Default variant
    pub const fn default(value: DefaultColumnConstraint) -> Self {
        Self::Default(value)
    }

    /// get Default variant
    pub const fn as_default(&self) -> Option<&DefaultColumnConstraint> {
        if let Self::Default(c) = self {
            Some(c)
        } else {
            None
        }
    }
}

// Analysis owned by the semantic definitions in this module.

impl DefaultColumnConstraint {
    pub(in crate::sem::column_constraints) fn from_ast_expression(
        context: &mut impl ParseContext,
        table: &TableName,
        column: &ColumnName,
        expression: &AstExpr,
    ) -> Result<Self, AnalysisError> {
        Ok(Self::new(
            table,
            column,
            SemScalarExpr::from_ast_in(expression, context, &ScalarAnalysisContext::default())?,
        ))
    }
}
