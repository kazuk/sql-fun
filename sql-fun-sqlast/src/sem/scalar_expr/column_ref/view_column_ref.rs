use crate::sem::scalar_expr::column_ref::{ColumnReference, ColumnReferenceExpr};
use crate::sem::{
    ColumnDefinition, ColumnName, Nullable, TypeReference, ViewName, data_source::AliasName,
};

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ViewColumnRef {
    alias: AliasName,
    column: ColumnName,
    view_name: ViewName,
    column_def: Option<ColumnDefinition>,
    datasource_nullable: Nullable,
}

impl ColumnReference for ViewColumnRef {
    fn get_type(&self) -> Option<TypeReference> {
        if let Some(col_def) = &self.column_def {
            col_def.get_type_cloned()
        } else {
            None
        }
    }

    fn is_not_null(&self) -> Option<bool> {
        if self.datasource_nullable {
            return Some(false);
        }
        let Some(column_def) = &self.column_def else {
            // column definition not resolved, returns Unknown
            return None;
        };
        // returns column nullability
        column_def.is_not_null()
    }

    fn get_column_def(&self) -> &Option<ColumnDefinition> {
        &self.column_def
    }

    fn get_column_name(&self) -> &ColumnName {
        &self.column
    }
}

impl ViewColumnRef {
    pub(super) const fn projection_alias(&self) -> &str {
        self.alias.as_str()
    }

    pub(super) fn with_reference_name(mut self, alias: &AliasName, column: &ColumnName) -> Self {
        self.alias = alias.clone();
        self.column = column.clone();
        self
    }

    pub(super) fn canonicalize_grouping_key(&mut self) {
        self.column_def = None;
    }
    pub(super) fn belongs_to(&self, relation: &crate::sem::FullName) -> bool {
        self.view_name.full_name() == relation
    }

    fn new(
        alias: &AliasName,
        column: &ColumnName,
        view_name: &ViewName,
        column_def: &Option<ColumnDefinition>,
        datasource_nullable: Nullable,
    ) -> Self {
        Self {
            alias: alias.clone(),
            column: column.clone(),
            view_name: view_name.clone(),
            column_def: column_def.clone(),
            datasource_nullable,
        }
    }
}

impl ColumnReferenceExpr {
    pub(crate) fn view_column(
        alias: &AliasName,
        column: &ColumnName,
        view: &ViewName,
        definition: Option<&ColumnDefinition>,
        nullable: Nullable,
    ) -> Self {
        Self::ViewColumn(ViewColumnRef::new(
            alias,
            column,
            view,
            &definition.cloned(),
            nullable,
        ))
    }
}
