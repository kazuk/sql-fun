use turso_parser::ast::ColumnDefinition as ColumnDefinitionIn;

use crate::syn::sqlite::{Name, NamedColumnConstraint, Type};

/// column definition
pub struct ColumnDefinition {
    col_name: Name,
    col_type: Option<Type>,
    constraints: Vec<NamedColumnConstraint>,
}

impl From<ColumnDefinitionIn> for ColumnDefinition {
    fn from(value: ColumnDefinitionIn) -> Self {
        Self {
            col_name: Name::from(value.col_name),
            col_type: value.col_type.map(Type::from),
            constraints: value
                .constraints
                .into_iter()
                .map(NamedColumnConstraint::from)
                .collect(),
        }
    }
}

impl ColumnDefinition {
    /// column name
    pub fn column_name(&self) -> &Name {
        &self.col_name
    }

    /// column type
    pub fn column_type(&self) -> Option<&Type> {
        self.col_type.as_ref()
    }

    /// constraints
    pub fn constraints(&self) -> &[NamedColumnConstraint] {
        self.constraints.as_slice()
    }
}
