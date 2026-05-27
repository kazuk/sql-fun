use crate::{
    engine_specific::TursoMaterializedViewFeature,
    sem::AnalysisError,
    syn::sqlite::{IndexedColumn, QualifiedName, Select},
};

/// create view statement
#[derive(derive_getters::Getters)]
pub struct CreateView {
    temporary: bool,
    if_not_exists: bool,
    view_name: QualifiedName,
    columns: Vec<IndexedColumn>,
    select: Select,
}

impl CreateView {
    /// create instance
    pub fn new(
        temporary: bool,
        if_not_exists: bool,
        view_name: QualifiedName,
        columns: Vec<IndexedColumn>,
        select: Select,
    ) -> Self {
        Self {
            temporary,
            if_not_exists,
            view_name,
            columns,
            select,
        }
    }
}

/// create materialized view statement
///
/// This AST is turso experimental feature
/// accessing `view_name` reports notify `engine_specific::TursoMaterializedViewFeature`
///
#[derive(derive_getters::Getters)]
pub struct CreateMaterializedView {
    if_not_exists: bool,
    #[getter(skip)]
    view_name: QualifiedName,
    columns: Vec<IndexedColumn>,
    select: Select,
}

impl CreateMaterializedView {
    /// create instance
    pub fn new(
        if_not_exists: bool,
        view_name: QualifiedName,
        columns: Vec<IndexedColumn>,
        select: Select,
    ) -> Self {
        Self {
            if_not_exists,
            view_name,
            columns,
            select,
        }
    }

    /// get view_name
    pub fn view_name<Gate>(&self, gate: &mut Gate) -> Result<&QualifiedName, AnalysisError>
    where
        Gate: TursoMaterializedViewFeature,
    {
        gate.create_materialized_view_used(self)?;
        Ok(&self.view_name)
    }
}

/// drop view statement
#[derive(derive_getters::Getters)]
pub struct DropView {
    if_exists: bool,
    view_name: QualifiedName,
}

impl DropView {
    /// create instance
    pub fn new(if_exists: bool, view_name: QualifiedName) -> Self {
        Self {
            if_exists,
            view_name,
        }
    }
}
