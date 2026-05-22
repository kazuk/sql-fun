use crate::{engine_specific::TursoArrayFeature, sem::AnalysisError};

use super::Expr;

/// array expression
#[derive(Clone, Debug)]
pub struct Array(Vec<Box<Expr>>);

impl Array {
    /// create instance
    pub fn new(elements: Vec<Expr>) -> Self {
        Self(elements.into_iter().map(Box::new).collect())
    }

    /// enumerate elements
    pub fn elements<Gate>(
        &self,
        gate: &mut Gate,
    ) -> Result<impl Iterator<Item = &Expr>, AnalysisError>
    where
        Gate: TursoArrayFeature,
    {
        gate.array_element_access(&self)?;
        Ok(self.0.iter().map(|v| v.as_ref()))
    }
}

impl Expr {
    /// create Array variant
    pub fn array<E>(elements: Vec<E>) -> Self
    where
        E: Into<Expr>,
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
        gate.subscript_base_access(&self)?;
        Ok(&self.base)
    }

    /// get a index
    pub fn index<Gate>(&self, gate: &mut Gate) -> Result<&Expr, AnalysisError>
    where
        Gate: TursoArrayFeature,
    {
        gate.subscript_index_access(&self)?;
        Ok(&self.index)
    }
}

impl Expr {
    /// create Subscript variant
    pub fn subscript<B, I>(base: B, index: I) -> Self
    where
        B: Into<Expr>,
        I: Into<Expr>,
    {
        Self::Subscript(Subscript::new(base.into(), index.into()))
    }
}
