//! Transitional loaded representations of legacy non-base definitions.
//!
//! These wrappers preserve the complete legacy payload until category-specific
//! component models and their identity rules have been specified. They do not
//! take over storage or change existing semantic analyzer behavior.

use std::any::Any;
use crate::sem::{
    ColumnCollection, CreateCompositeType, CreateDomain, CreateEnum, TableName,
};
use super::{TypeDefinition, TypeReference};

/// A loaded enumeration definition.
#[derive(Debug)]
pub struct EnumType {
    definition: CreateEnum,
}
impl EnumType {
    /// Retain an existing enumeration definition.
    #[must_use]
    pub const fn new(definition: CreateEnum) -> Self { Self { definition } }
    /// Original complete definition.
    #[must_use]
    pub const fn definition(&self) -> &CreateEnum { &self.definition }
}
impl TypeDefinition for EnumType {
    fn as_any(&self) -> &dyn Any { self }
}

/// A loaded named composite definition.
#[derive(Debug)]
pub struct CompositeType {
    definition: CreateCompositeType,
}
impl CompositeType {
    /// Retain an existing composite definition.
    #[must_use]
    pub const fn new(definition: CreateCompositeType) -> Self { Self { definition } }
    /// Original complete definition.
    #[must_use]
    pub const fn definition(&self) -> &CreateCompositeType { &self.definition }
}
impl TypeDefinition for CompositeType {
    fn as_any(&self) -> &dyn Any { self }
}

/// A loaded domain definition.
#[derive(Debug)]
pub struct DomainType {
    definition: CreateDomain,
}
impl DomainType {
    /// Retain an existing domain definition.
    #[must_use]
    pub const fn new(definition: CreateDomain) -> Self { Self { definition } }
    /// Original complete definition.
    #[must_use]
    pub const fn definition(&self) -> &CreateDomain { &self.definition }
}
impl TypeDefinition for DomainType {
    fn as_any(&self) -> &dyn Any { self }
}

/// A relation-backed row type, including its original reference.
#[derive(Debug)]
pub struct TableRowType {
    table: TableName,
    reference: TypeReference,
}
impl TableRowType {
    /// Retain an existing relation type and its reference.
    #[must_use]
    pub fn new(table: TableName, reference: TypeReference) -> Self {
        Self { table, reference }
    }
    /// Referenced relation name.
    #[must_use]
    pub const fn table(&self) -> &TableName { &self.table }
    /// Original reference loaded as a type value.
    #[must_use]
    pub fn reference(&self) -> &dyn TypeDefinition { self.reference.as_ref() }
}
impl TypeDefinition for TableRowType {
    fn as_any(&self) -> &dyn Any { self }
}

/// A loaded anonymous record definition with its original column collection.
#[derive(Debug)]
pub struct ImplicitCompositeType {
    columns: ColumnCollection,
}
impl ImplicitCompositeType {
    /// Retain an existing anonymous composite definition.
    #[must_use]
    pub const fn new(columns: ColumnCollection) -> Self { Self { columns } }
    /// Original complete column collection.
    #[must_use]
    pub const fn columns(&self) -> &ColumnCollection { &self.columns }
}
impl TypeDefinition for ImplicitCompositeType {
    fn as_any(&self) -> &dyn Any { self }
}
