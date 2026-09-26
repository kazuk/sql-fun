use crate::syn::sqlite::expr::Expr;
use crate::{engine_specific::TursoArrayFeature, sem::AnalysisError};

/// array expression
#[derive(Clone, Debug)]
pub struct Array(Vec<Expr>);

impl Array {
    pub(crate) fn syntax_elements(&self) -> &[Expr] {
        &self.0
    }

    /// create instance
    pub const fn new(elements: Vec<Expr>) -> Self {
        Self(elements)
    }

    /// enumerate elements
    pub fn elements<Gate>(
        &self,
        gate: &mut Gate,
    ) -> Result<impl Iterator<Item = &Expr>, AnalysisError>
    where
        Gate: TursoArrayFeature,
    {
        gate.array_element_access(self)?;
        Ok(self.0.iter())
    }
}

impl Expr {
    /// create Array variant
    pub fn array<E>(elements: Vec<E>) -> Self
    where
        E: Into<Self>,
    {
        Self::Array(Array::new(elements.into_iter().map(|v| v.into()).collect()))
    }
}

/// subscript expression
#[derive(Clone, Debug)]
pub struct Subscript {
    base: Box<Expr>,
    index: Box<Expr>,
}

impl Subscript {
    pub(crate) fn syntax_base(&self) -> &Expr {
        &self.base
    }

    pub(crate) fn syntax_index(&self) -> &Expr {
        &self.index
    }

    /// create instance
    pub fn new(base: Expr, index: Expr) -> Self {
        Self {
            base: Box::new(base),
            index: Box::new(index),
        }
    }

    /// get a base
    pub fn base<Gate>(&self, gate: &mut Gate) -> Result<&Expr, AnalysisError>
    where
        Gate: TursoArrayFeature,
    {
        gate.subscript_base_access(self)?;
        Ok(&self.base)
    }

    /// get a index
    pub fn index<Gate>(&self, gate: &mut Gate) -> Result<&Expr, AnalysisError>
    where
        Gate: TursoArrayFeature,
    {
        gate.subscript_index_access(self)?;
        Ok(&self.index)
    }
}

impl Expr {
    /// create Subscript variant
    pub fn subscript<B, I>(base: B, index: I) -> Self
    where
        B: Into<Self>,
        I: Into<Self>,
    {
        Self::Subscript(Subscript::new(base.into(), index.into()))
    }
}
