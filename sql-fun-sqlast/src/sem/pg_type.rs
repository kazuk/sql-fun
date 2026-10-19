mod category;
mod concrete_type;
mod dynamic_type;
mod type_ref;

pub use self::{category::TypeCategory, type_ref::TypeReference};

use crate::sem::{
    ColumnCollection, ColumnName, CreateCompositeType, CreateDomain, CreateEnum, FullName,
    LocalName, ParseContext, SchemaName, TableName,
};

/// type definitions
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum TypeDefinition {
    /// an enum type
    Enum(CreateEnum),
    /// a composite type
    CompositeType(CreateCompositeType),
    /// a domain type
    Domain(CreateDomain),
    /// a builtin type
    Builtin(BuiltinTypeDefinition),
    /// table
    TableRef(TableName, TypeReference),
    /// implicit composite type
    ImplicitComposite(ColumnCollection),
}

/// Catalog-loaded nominal type and its optional PostgreSQL classification.
/// The flattened reference retains the existing serialized reference shape.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct BuiltinTypeDefinition {
    #[serde(flatten)]
    reference: TypeReference,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    category: Option<TypeCategory>,
}

impl TypeDefinition {
    /// Construct a catalog type without inferring classification from its name.
    pub fn builtin_with_category(full_name: &FullName, category: Option<TypeCategory>) -> Self {
        Self::Builtin(BuiltinTypeDefinition {
            reference: TypeReference::concrete_type_ref(full_name, false),
            category,
        })
    }

    /// PostgreSQL classification from catalog metadata or verified DDL semantics.
    ///
    /// Missing metadata, unresolved domains, and non-PostgreSQL contexts
    /// return `None`, distinct from the PostgreSQL codes `X` and `U`.
    pub fn category(&self, context: &impl crate::sem::BaseParseContext) -> Option<TypeCategory> {
        if crate::ast::Dialect::from(context.metadata()) != crate::ast::Dialect::postgres() {
            return None;
        }
        match self {
            Self::Builtin(definition) => definition.category,
            Self::Enum(_) => TypeCategory::try_from('E').ok(),
            Self::CompositeType(_) | Self::TableRef(_, _) | Self::ImplicitComposite(_) => {
                TypeCategory::try_from('C').ok()
            }
            Self::Domain(definition) => definition.category(),
        }
    }

    pub(crate) const fn is_builtin(&self) -> bool {
        matches!(self, Self::Builtin(_))
    }

    pub(crate) const fn is_relation_type(&self) -> bool {
        matches!(self, Self::TableRef(_, _))
    }
    pub(crate) const fn is_domain(&self) -> bool {
        matches!(self, Self::Domain(_))
    }
    /// Ownership retained by a standalone type definition.
    /// Table row types obtain ownership from their referenced relation.
    pub const fn owner(&self) -> Option<&crate::sem::OwnerSpecification> {
        match self {
            Self::Enum(definition) => definition.owner(),
            Self::CompositeType(definition) => definition.owner(),
            Self::Domain(definition) => definition.owner(),
            _ => None,
        }
    }
    pub(crate) fn change_owner(&mut self, owner: &crate::sem::OwnerSpecification) -> bool {
        match self {
            Self::Enum(definition) => definition.change_owner(owner),
            Self::CompositeType(definition) => definition.change_owner(owner),
            Self::Domain(definition) => definition.change_owner(owner),
            _ => return false,
        }
        true
    }

    pub(crate) const fn domain_base(&self) -> Option<&FullName> {
        if let Self::Domain(domain) = self {
            domain.base_type()
        } else {
            None
        }
    }
    pub(crate) const fn user_type_category(&self) -> Option<char> {
        match self {
            Self::Enum(_) => Some('E'),
            Self::CompositeType(_) | Self::TableRef(..) | Self::ImplicitComposite(_) => Some('C'),
            _ => None,
        }
    }
    pub(crate) const fn has_structural_ordering(&self) -> bool {
        self.user_type_category().is_some()
    }

    /// type is array?
    #[must_use]
    pub fn is_array(&self) -> Option<bool> {
        match self {
            Self::Enum(_) => Some(false),
            Self::CompositeType(_) => Some(false),
            Self::Domain(create_domain) => create_domain.type_ref().is_array(),
            Self::Builtin(definition) => definition.reference.is_array(),
            Self::TableRef(_, type_reference) => type_reference.is_array(),
            Self::ImplicitComposite(_) => Some(false),
        }
    }

    /// table definition reference
    #[must_use]
    pub fn table_ref(table_name: &TableName) -> Self {
        let tr = TypeReference::from_full_name(table_name.full_name().clone());
        Self::TableRef(table_name.clone(), tr)
    }

    /// get reference of type
    #[must_use]
    pub fn type_reference(&self) -> Option<&TypeReference> {
        match self {
            Self::Builtin(definition) => Some(&definition.reference),
            Self::Domain(ct) => Some(ct.type_ref()),
            Self::CompositeType(ct) => ct.type_ref().as_ref(),
            Self::TableRef(_, tr) => Some(tr),
            _ => todo!("self:?"),
        }
    }

    /// get a name of type
    #[must_use]
    pub fn full_name(&self) -> Option<&FullName> {
        match self {
            Self::Enum(create_enum) => Some(create_enum.name()),
            Self::CompositeType(create_composite_type) => create_composite_type.name(),
            Self::Domain(create_domain) => Some(create_domain.name()),
            Self::Builtin(definition) => Some(definition.reference.full_name()),
            Self::TableRef(t, _) => Some(t.full_name()),
            Self::ImplicitComposite(_) => todo!(),
        }
    }

    /// get builtin type by name
    #[must_use]
    pub fn builtin(full_name: &FullName) -> Self {
        Self::builtin_with_category(full_name, None)
    }

    /// check type compatibility
    #[must_use]
    pub fn compatible_with(&self, other: &Self) -> bool {
        if self.full_name() == other.full_name() {
            true
        } else {
            todo!()
        }
    }

    /// test a type is composite type
    #[must_use]
    pub const fn is_composite_type(&self) -> bool {
        matches!(&self, Self::CompositeType(_))
    }

    /// enumelate members
    #[must_use]
    pub fn get_composite_type_columns(&self) -> ColumnCollection {
        match &self {
            Self::CompositeType(ct) => ct.get_column_collection(),
            _ => unreachable!(),
        }
    }

    /// get member type
    pub fn get_member_type<TParseContext>(
        &self,
        context: &TParseContext,
        member: &str,
    ) -> Option<TypeReference>
    where
        TParseContext: ParseContext,
    {
        self.member_type_from_catalog(context, member)
    }

    pub(crate) fn member_type_from_catalog<TCatalog: crate::sem::BaseParseContext>(
        &self,
        context: &TCatalog,
        member: &str,
    ) -> Option<TypeReference> {
        match self {
            Self::Enum(_create_enum) => todo!(),
            Self::CompositeType(create_composite_type) => {
                create_composite_type.get_member(member)?.get_type()
            }
            Self::Domain(_create_domain) => todo!(),
            Self::Builtin(_type_reference) => None,
            Self::TableRef(table_name, _type_reference) => context
                .get_table(table_name)?
                .get_column_def(&ColumnName::from(member))?
                .get_type_cloned(),
            Self::ImplicitComposite(cols) => {
                let cd = cols.get_column_def(&ColumnName::from(member))?;
                cd.get_type_cloned()
            }
        }
    }
}

impl From<&CreateEnum> for TypeDefinition {
    fn from(value: &CreateEnum) -> Self {
        Self::Enum(value.clone())
    }
}

impl From<&CreateCompositeType> for TypeDefinition {
    fn from(value: &CreateCompositeType) -> Self {
        Self::CompositeType(value.clone())
    }
}

impl From<&CreateDomain> for TypeDefinition {
    fn from(value: &CreateDomain) -> Self {
        Self::Domain(value.clone())
    }
}

/// Well-known Postgres Built-In types
pub struct PgBuiltInType {}

impl PgBuiltInType {
    fn builtin_reference(builtin_type_name: &'static str) -> TypeReference {
        TypeReference::from_full_name(FullName::new(
            &None,
            &Some(SchemaName::from("pg_catalog")),
            &LocalName::from(builtin_type_name),
        ))
    }

    /// get built-in boolean type reference
    #[must_use]
    pub fn bool() -> TypeReference {
        Self::builtin_reference("bool")
    }

    /// get built-in `int4` type reference
    #[must_use]
    pub fn int2() -> TypeReference {
        Self::builtin_reference("int4")
    }

    /// get built-in `int4` type reference
    #[must_use]
    pub fn int4() -> TypeReference {
        Self::builtin_reference("int4")
    }

    /// get built-in `int4` type reference
    #[must_use]
    pub fn int8() -> TypeReference {
        Self::builtin_reference("int8")
    }

    /// get built-in `numeric` type reference
    #[must_use]
    pub fn numeric() -> TypeReference {
        Self::builtin_reference("numeric")
    }

    /// get built-in `text` type reference
    #[must_use]
    pub fn text() -> TypeReference {
        Self::builtin_reference("text")
    }

    /// get built-in `varbit` type reference
    #[must_use]
    pub fn varbit() -> TypeReference {
        Self::builtin_reference("varbit")
    }
}

#[cfg(test)]
mod tests {
    use crate::sem::FullName;
    use crate::syn::NodeListOpt;

    fn build_string_node_list_opt(strings: &[&str]) -> NodeListOpt {
        use crate::syn::StringBuilder;

        let mut string_values = Vec::new();
        for string in strings {
            let string_value = StringBuilder::default()
                .sval(String::from(*string))
                .build()
                .unwrap();
            let string_value = crate::syn::String::from(string_value);
            let string_node = crate::syn::NodeInner::from(string_value);
            let string_node = crate::syn::Node::from(string_node);
            string_values.push(string_node);
        }
        NodeListOpt::from(string_values)
    }

    #[test]
    fn test_try_from_nodelistopt_two_elements() {
        let nlo = build_string_node_list_opt(&["schema", "name"]);
        let qt = FullName::try_from(nlo).unwrap();
        assert!(qt.catalog().is_none());
        assert_eq!(qt.schema().unwrap().as_str(), "schema");
        assert_eq!(qt.name().as_str(), "name");
    }

    #[test]
    fn test_try_from_nodelistopt_three_elements() {
        let nlo = build_string_node_list_opt(&["cat", "sch", "nm"]);
        let qt = FullName::try_from(nlo).unwrap();
        assert_eq!(qt.catalog().unwrap().as_str(), "cat");
        assert_eq!(qt.schema().unwrap().as_str(), "sch");
        assert_eq!(qt.name().as_str(), "nm");
    }
}
