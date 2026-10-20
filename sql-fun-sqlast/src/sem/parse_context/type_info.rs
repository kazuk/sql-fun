use crate::sem::{
    CreateCompositeType, CreateDomain, CreateEnum, FullName, TableName, TypeDefinition,
    TypeReference,
};

/// get type definition
pub trait TypeInfoRead {
    /// get type by name
    fn get_type_impl(&self, key: &FullName) -> Option<&TypeDefinition>;

    /// Whether a builtin has a default B-tree comparison implementation,
    /// including PostgreSQL binary-compatible operator-class input types.
    /// `Some(true)` and `Some(false)` are catalog-backed answers; `None` means
    /// the provider lacks the knowledge needed to answer for this identity.
    fn builtin_type_ordered(&self, name: &FullName) -> Option<bool>;

    /// get item type for array type
    fn get_array_item_type(&self, type_name: &TypeReference) -> Option<TypeReference>;
}

/// store and update type info
pub trait TypeInfoWrite
where
    Self: Sized,
{
    /// insert table reference composite type
    fn insert_table_ref_type(&mut self, name: &TableName);

    /// register enum type
    fn insert_create_composite_type(
        &mut self,
        name: &FullName,
        create_comp_type: &CreateCompositeType,
    );

    /// insert create domain statement
    fn insert_create_domain(&mut self, name: &FullName, create_domain: &CreateDomain);

    /// register enum type
    fn insert_create_enum(&mut self, name: &FullName, create_enum_type: &CreateEnum);
}
