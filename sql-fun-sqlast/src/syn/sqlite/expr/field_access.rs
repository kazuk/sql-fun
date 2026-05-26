use super::Expr;
use crate::syn::sqlite::Name;

use turso_parser::ast::FieldAccessResolution as FieldAccessResolutionIn;

/// field access resolution specifier
#[derive(Clone, Debug)]
pub enum FieldAccessResolution {
    /// struct field
    StructField(usize),
    /// union variant
    UnionVariant(u8),
}

impl FieldAccessResolution {
    /// get field index
    pub fn struct_field_index(&self) -> Option<usize> {
        match self {
            Self::StructField(v) => Some(*v),
            _ => None,
        }
    }

    /// get tag index
    pub fn union_tag_index(&self) -> Option<u8> {
        match self {
            Self::UnionVariant(v) => Some(*v),
            _ => None,
        }
    }
}

impl From<FieldAccessResolutionIn> for FieldAccessResolution {
    fn from(value: FieldAccessResolutionIn) -> Self {
        match value {
            FieldAccessResolutionIn::StructField { field_index } => Self::StructField(field_index),
            FieldAccessResolutionIn::UnionVariant { tag_index } => Self::UnionVariant(tag_index),
        }
    }
}

/// field access expression
#[derive(Clone, Debug, derive_getters::Getters)]
pub struct FieldAccess {
    base: Box<Expr>,
    field: Name,
    resolved: Option<FieldAccessResolution>,
}

impl FieldAccess {
    /// create instance
    pub fn new(base: Expr, field: Name, resolved: Option<FieldAccessResolution>) -> Self {
        Self {
            base: Box::new(base),
            field,
            resolved,
        }
    }
}

impl Expr {
    /// create FieldAccess variant
    pub fn field_access<E, N, FAR>(base: E, field: N, resolved: Option<FAR>) -> Self
    where
        E: Into<Self>,
        N: Into<Name>,
        FAR: Into<FieldAccessResolution>,
    {
        Self::FieldAccess(FieldAccess::new(
            base.into(),
            field.into(),
            resolved.map(|v| v.into()),
        ))
    }
}
