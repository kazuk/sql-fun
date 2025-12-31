use crate::sem::{ColumnName, SemScalarExpr, TypeReference};

/// column definitions
#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ColumnDefinition {
    name: Option<ColumnName>,
    type_ref: Option<TypeReference>,
    is_not_null: Option<bool>,
    default: Option<Box<SemScalarExpr>>,
}

impl Clone for ColumnDefinition {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            type_ref: self.type_ref.clone(),
            is_not_null: self.is_not_null,
            default: Default::default(),
        }
    }
}

impl ColumnDefinition {
    /// create instance
    pub fn new(
        name: &Option<ColumnName>,
        type_ref: Option<&TypeReference>,
        is_not_null: Option<bool>,
    ) -> Self {
        Self {
            name: name.clone(),
            type_ref: type_ref.cloned(),
            is_not_null,
            default: Default::default(),
        }
    }

    /// get name of column
    pub fn name(&self) -> &Option<ColumnName> {
        &self.name
    }

    /// get [TypeReference]
    pub fn get_type(&self) -> Option<TypeReference> {
        self.type_ref.clone()
    }

    /// column nullability
    pub fn is_not_null(&self) -> Option<bool> {
        self.is_not_null
    }

    /// apply default expression to column
    pub fn set_default(&mut self, scalar_expr: &SemScalarExpr) {
        self.default = Some(Box::new(scalar_expr.clone()))
    }
}
