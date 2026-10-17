use crate::sem::scalar_expr::column_ref::{ColumnReference, ColumnReferenceExpr};
use crate::sem::{
    AnalysisError, AnalysisProblem, ColumnDefinition, ColumnName, Nullable, SemScalarExpr,
    TypeReference, ViewName, data_source::AliasName,
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

    pub(crate) fn view_source_ref<'context, TParseContext>(
        context: &'context mut TParseContext,
        alias: &AliasName,
        column: &ColumnName,
        view_name: &ViewName,
        nullability: Nullable,
    ) -> Result<(SemScalarExpr, &'context mut TParseContext), AnalysisError>
    where
        TParseContext: crate::sem::ParseContext,
    {
        let Some(view) = context.get_view(view_name) else {
            AnalysisError::raise_unexpected_input(&format!(
                "view {view_name} not found in context"
            ))?
        };

        let column_def = view.get_column_def(column);
        if column_def.is_none() {
            context.report_analysis_problem(AnalysisProblem::column_not_found_in_view(
                view_name, column,
            ))?;
        }
        let col_ref = Self::new(alias, column, view_name, &column_def, nullability);
        Ok((
            SemScalarExpr::ColumnRef(ColumnReferenceExpr::ViewColumn(col_ref)),
            context,
        ))
    }
}

impl ColumnReferenceExpr {
    pub(crate) fn view_source_ref<'context, TParseContext>(
        context: &'context mut TParseContext,
        alias: &AliasName,
        column: &ColumnName,
        view_name: &ViewName,
        nullability: Nullable,
    ) -> Result<(SemScalarExpr, &'context mut TParseContext), AnalysisError>
    where
        TParseContext: crate::sem::ParseContext,
    {
        ViewColumnRef::view_source_ref(context, alias, column, view_name, nullability)
    }
}
