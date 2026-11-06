use crate::{
    ast::{Expr as AstExpr, GeneratedColumnMode},
    names::{ColumnName, TableName},
    sem::{
        AnalysisError, ColumnConstraint, ColumnConstraintKind, ColumnConstraintTrait, ColumnRename,
        ParseContext, ScalarAnalysisContext, SemScalarExpr, traversal::SemanticVisitorMut,
    },
};

/// generated column / computed column constraint
#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub struct GeneratedColumnConstraint {
    table_name: TableName,
    column_name: ColumnName,
    expr: SemScalarExpr,
    generated_when: Option<GeneratedColumnMode>,
}

impl GeneratedColumnConstraint {
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

    pub(crate) fn is_stored(&self) -> bool {
        self.generated_when == Some(GeneratedColumnMode::stored())
    }
    pub(crate) fn is_virtual(&self) -> bool {
        self.generated_when == Some(GeneratedColumnMode::r#virtual())
    }
    /// create instance
    pub fn new(
        table_name: &TableName,
        column_name: &ColumnName,
        expr: SemScalarExpr,
        generated_when: Option<GeneratedColumnMode>,
    ) -> Self {
        Self {
            table_name: table_name.clone(),
            column_name: column_name.clone(),
            expr,
            generated_when,
        }
    }
}

impl ColumnConstraintTrait for GeneratedColumnConstraint {
    fn kind(&self) -> ColumnConstraintKind {
        ColumnConstraintKind::Generated
    }

    fn table_name(&self) -> &TableName {
        &self.table_name
    }

    fn column_name(&self) -> &ColumnName {
        &self.column_name
    }
}

impl ColumnConstraint {
    /// create Generated variant
    pub const fn generated(value: GeneratedColumnConstraint) -> Self {
        Self::Generated(value)
    }

    /// get Generated variant
    pub const fn as_generated(&self) -> Option<&GeneratedColumnConstraint> {
        if let Self::Generated(c) = self {
            Some(c)
        } else {
            None
        }
    }
}

// Analysis owned by the semantic definitions in this module.

impl GeneratedColumnConstraint {
    pub(in crate::sem::column_constraints) fn from_ast_expression(
        context: &mut impl ParseContext,
        table: &TableName,
        column: &ColumnName,
        expression: &AstExpr,
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
