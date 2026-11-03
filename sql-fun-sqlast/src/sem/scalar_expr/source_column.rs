use crate::{
    names::{AliasName, ColumnName},
    sem::{
        AnalysisError, ColumnDefinition, SemScalarExpr,
        traversal::{SemanticVisitor, SemanticVisitorMut},
    },
};

/// A completed FROM-column binding backed by an analyzed expression.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SourceColumnExpr {
    alias: AliasName,
    name: ColumnName,
    value: SemScalarExpr,
    nullable: bool,
}

impl SourceColumnExpr {
    pub(super) fn definition(&self) -> ColumnDefinition {
        ColumnDefinition::new(
            &Some(self.name.clone()),
            self.value.get_type().as_ref(),
            self.is_not_null(),
            &[],
        )
    }

    /// Analyzed source value; its type domain remains linked to the column.
    pub const fn value(&self) -> &SemScalarExpr {
        &self.value
    }
    /// Visible source qualifier after aliases are applied.
    pub const fn alias(&self) -> &AliasName {
        &self.alias
    }
    /// Visible column name after source aliases are applied.
    pub const fn name(&self) -> &ColumnName {
        &self.name
    }
    pub(super) fn is_not_null(&self) -> Option<bool> {
        if self.nullable {
            Some(false)
        } else {
            self.value.is_not_null()
        }
    }
    pub(super) fn with_reference_name(mut self, alias: &AliasName, name: &ColumnName) -> Self {
        self.alias = alias.clone();
        self.name = name.clone();
        self
    }
    pub(super) const fn with_nullability(mut self, nullable: bool) -> Self {
        self.nullable |= nullable;
        self
    }
    pub(super) fn walk<V: SemanticVisitorMut>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        if visitor.walk_source_column_values() {
            self.value.walk(visitor)?;
        }
        Ok(())
    }
    pub(super) fn visit<V: SemanticVisitor>(&self, visitor: &mut V) -> Result<(), AnalysisError> {
        if visitor.walk_source_column_values() {
            self.value.visit(visitor)?;
        }
        Ok(())
    }
}
impl SemScalarExpr {
    pub(crate) fn source_column(value: &Self, alias: &AliasName, name: &ColumnName) -> Self {
        Self::SourceColumn(Box::new(SourceColumnExpr {
            alias: alias.clone(),
            name: name.clone(),
            value: value.clone(),
            nullable: false,
        }))
    }
}

impl SemScalarExpr {
    /// Completed expression-backed FROM-column binding.
    pub fn as_source_column(&self) -> Option<&SourceColumnExpr> {
        match self {
            Self::SourceColumn(value) => Some(value),
            _ => None,
        }
    }
}
