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
    fn builtin_type_ordered(&self, _name: &FullName) -> bool {
        false
    }

    /// Complete resolved base of a domain, preserving array qualification.
    /// The default adapts legacy scalar declarations; catalogs retaining array
    /// bases override it with their complete resolved type facts.
    fn domain_base_type(&self, name: &FullName) -> Option<TypeReference> {
        self.get_type_impl(name)?
            .domain_base()
            .cloned()
            .map(TypeReference::from_full_name)
    }

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

    /// Retain the complete domain base alongside the existing declaration model.
    fn insert_create_domain_with_base(
        &mut self,
        name: &FullName,
        domain: &CreateDomain,
        base: Option<&TypeReference>,
    );

    /// register enum type
    fn insert_create_enum(&mut self, name: &FullName, create_enum_type: &CreateEnum);
}
