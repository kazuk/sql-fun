mod array;
mod category;
mod concrete_type;
mod dynamic_type;
mod type_ref;

pub use self::array::ArrayTypeDefinition;
pub use self::{category::TypeCategory, type_ref::TypeReference};

use crate::context::BaseParseContext;
use crate::{
    context::ParseContext,
    sem::{
        ColumnCollection, ColumnName, CreateCompositeType, CreateDomain, CreateEnum, FullName,
        LocalName, OwnerSpecification, SchemaName, TableName,
    },
};
use std::collections::HashSet;

/// type definitions
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum TypeDefinition {
    /// An array whose element retains its structural type identity.
    Array(ArrayTypeDefinition),
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

/// Catalog-loaded nominal type and its optional PostgreSQL facts.
/// The flattened reference retains the existing serialized reference shape.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct BuiltinTypeDefinition {
    #[serde(flatten)]
    reference: TypeReference,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    category: Option<TypeCategory>,
    #[serde(deserialize_with = "deserialize_collatable")]
    collatable: Option<bool>,
}

impl std::fmt::Display for TypeDefinition {
    /// Diagnostic spelling; anonymous structures have no canonical catalog text.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Array(array) => write!(f, "{}[]", array.element()),
            Self::ImplicitComposite(columns) => columns.fmt_type_shape(f),
            Self::CompositeType(composite) if composite.name().is_none() => {
                composite.get_column_collection().fmt_type_shape(f)
            }
            _ => {
                if let Some(reference) = self.type_reference() {
                    write!(f, "{reference}")
                } else {
                    f.write_str("unnamed SQL type")
                }
            }
        }
    }
}

#[cfg(test)]
mod builtin_type_definition_tests {
    use super::TypeDefinition;
    use crate::sem::{FullName, TypeReference};

    #[rstest::rstest]
    fn nullable_fact_survives_serialization(
        #[values(Some(true), Some(false), None)] expected: Option<bool>,
    ) {
        let definition =
            TypeDefinition::builtin(&FullName::with_schema("public", "custom"), expected);
        let context = crate::test_helpers::TestParseContext::default();
        let serialized = serde_json::to_value(&definition).unwrap();
        assert!(serialized["Builtin"].get("collatable").is_some());
        assert_eq!(
            serialized["Builtin"]["collatable"],
            serde_json::to_value(expected).unwrap()
        );
        let restored: TypeDefinition = serde_json::from_value(serialized).unwrap();
        assert_eq!(restored, definition);
        assert_eq!(
            TypeReference::dynamic(&restored).collation_capability(&context),
            expected
        );
    }

    #[test]
    fn semantic_definitions_also_require_the_fact_key() {
        let definition = TypeDefinition::builtin(&FullName::with_schema("public", "custom"), None);
        let mut serialized = serde_json::to_value(definition).unwrap();
        serialized["Builtin"]
            .as_object_mut()
            .unwrap()
            .remove("collatable");
        assert!(serde_json::from_value::<TypeDefinition>(serialized).is_err());
    }

    #[test]
    fn definition_preserves_flattened_reference_shape() {
        let name = FullName::with_schema("pg_catalog", "custom");
        let definition = TypeDefinition::builtin(&name, None);
        let json = serde_json::to_value(&definition).unwrap();
        assert!(json["Builtin"]["Concrete"].is_object());
        assert!(json["Builtin"].get("category").is_none());
        assert_eq!(
            serde_json::from_value::<TypeDefinition>(json).unwrap(),
            definition
        );
    }
}

// deserialize_with makes presence mandatory while accepting explicit JSON null.
pub(crate) fn deserialize_collatable<'de, D>(deserializer: D) -> Result<Option<bool>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    serde::Deserialize::deserialize(deserializer)
}

impl TypeDefinition {
    /// PostgreSQL classification from catalog metadata or verified DDL semantics.
    ///
    /// Missing metadata, unresolved domains, and non-PostgreSQL contexts
    /// return `None`, distinct from the PostgreSQL codes `X` and `U`.
    pub fn category(&self, context: &impl BaseParseContext) -> Option<TypeCategory> {
        if crate::ast::Dialect::from(context.metadata()) != crate::ast::Dialect::postgres() {
            return None;
        }
        self.intrinsic_category()
    }

    pub(crate) fn intrinsic_category(&self) -> Option<TypeCategory> {
        match self {
            Self::Array(_) => TypeCategory::try_from('A').ok(),
            Self::Builtin(definition) => definition.category,
            Self::Enum(_) => TypeCategory::try_from('E').ok(),
            Self::CompositeType(_) | Self::TableRef(_, _) | Self::ImplicitComposite(_) => {
                TypeCategory::try_from('C').ok()
            }
            Self::Domain(definition) => definition.category(),
        }
    }
}

#[cfg(test)]
mod category_tests {
    use super::TypeDefinition;
    use crate::{context::BaseParseContext, sem::FullName, test_helpers::TestParseContext};

    #[test]
    fn category_follows_selected_definition_and_search_path() {
        use crate::sem::{SchemaName, TypeCategory};

        let mut context = TestParseContext::default();
        context
            .set_get_search_path_result(&[SchemaName::from("second"), SchemaName::from("first")]);
        for (schema, code) in [("first", 'q'), ("second", '!')] {
            let name = FullName::with_schema(schema, "custom");
            context.set_get_type_impl_result(
                &name,
                &TypeDefinition::builtin_with_category(
                    &name,
                    Some(TypeCategory::try_from(code).unwrap()),
                    None,
                ),
            );
        }
        let selected = context
            .get_type(&FullName::from_local_name("custom"))
            .unwrap();
        assert_eq!(selected.category(&context).unwrap().code(), '!');
        let qualified = context
            .get_type(&FullName::with_schema("first", "custom"))
            .unwrap();
        assert_eq!(qualified.category(&context).unwrap().code(), 'q');
    }

    #[test]
    #[cfg(feature = "sqlite")]
    fn postgres_metadata_does_not_classify_sqlite_types() {
        let name = FullName::with_schema("pg_catalog", "int4");
        let category = crate::sem::TypeCategory::try_from('N').unwrap();
        let definition = TypeDefinition::builtin_with_category(&name, Some(category), None);
        let context = TestParseContext::default().with_dialect(crate::ast::Dialect::sqlite());
        assert_eq!(definition.category(&context), None);
    }
}

impl TypeDefinition {
    fn collation_capability_in(
        &self,
        context: &impl ParseContext,
        visited: &mut HashSet<FullName>,
    ) -> Option<bool> {
        match self {
            Self::Builtin(definition) => definition.collatable,
            Self::Array(array) => array.element().collation_capability_in(context, visited),
            Self::Domain(domain) => domain
                .base_type()?
                .collation_capability_in(context, visited),
            Self::Enum(_)
            | Self::CompositeType(_)
            | Self::TableRef(_, _)
            | Self::ImplicitComposite(_) => Some(false),
        }
    }
}

#[cfg(test)]
mod definition_collation_tests {
    use crate::{
        sem::{
            ColumnCollection, CompositeFieldCollection, CreateCompositeType, CreateEnum, FullName,
            TableName, TypeDefinition, TypeReference,
        },
        test_helpers::TestParseContext,
    };

    #[test]
    fn structural_rows_are_not_collatable() {
        let context = TestParseContext::default();
        let composite = CreateCompositeType::from_fields(&CompositeFieldCollection::default());
        let reference = TypeReference::dynamic(&TypeDefinition::from(&composite));
        assert_eq!(reference.collation_capability(&context), Some(false));
        let row = TypeReference::dynamic(&TypeDefinition::implicit_composite(
            &ColumnCollection::default(),
        ));
        assert_eq!(row.collation_capability(&context), Some(false));
        let enumeration =
            CreateEnum::new(&FullName::with_schema("public", "enum"), &["a".to_owned()]);
        let enumeration = TypeReference::dynamic(&TypeDefinition::from(&enumeration));
        assert_eq!(enumeration.collation_capability(&context), Some(false));
        let table = TableName::from(FullName::with_schema("public", "table"));
        let row = TypeReference::dynamic(&TypeDefinition::table_ref(&table));
        assert_eq!(row.collation_capability(&context), Some(false));
    }
}

impl TypeDefinition {
    pub(crate) fn rename_relation(
        &mut self,
        old: &crate::sem::TableName,
        new: &crate::sem::TableName,
    ) {
        match self {
            Self::Array(value) => value.rename_relation(old, new),
            Self::CompositeType(value) => value.rename_relation(old, new),
            Self::Domain(value) => value.rename_relation(old, new),
            Self::TableRef(table, ty) => {
                table.rename_reference(old, new);
                ty.rename_relation(old, new);
            }
            Self::ImplicitComposite(columns) => columns.rename_relation(old, new),
            Self::Enum(_) | Self::Builtin(_) => {}
        }
    }

    pub(crate) fn implicit_composite(columns: &ColumnCollection) -> Self {
        Self::ImplicitComposite(columns.clone())
    }

    /// Define a structural array without requiring a catalog name for its element.
    #[must_use]
    pub fn array(element: &TypeReference) -> Self {
        Self::Array(ArrayTypeDefinition::new(element))
    }

    pub(crate) const fn array_element(&self) -> Option<&TypeReference> {
        if let Self::Array(array) = self {
            Some(array.element())
        } else {
            None
        }
    }

    pub(crate) fn matches_structure(&self, other: &Self, context: &impl BaseParseContext) -> bool {
        self.matches_structure_with(other, |left, right| left.matches_structure(right, context))
    }

    pub(crate) fn matches_structure_with(
        &self,
        other: &Self,
        matches: impl Fn(&TypeReference, &TypeReference) -> bool,
    ) -> bool {
        if self == other {
            return true;
        }
        // Anonymous record descriptors are positional. A named composite
        // retains nominal identity; equal field layouts do not authorize an
        // implicit conversion from an anonymous record to that named type.
        match (self.structural_columns(), other.structural_columns()) {
            (Some(left), Some(right)) => left.matches_type_structure_with(&right, matches),
            _ => false,
        }
    }

    fn structural_columns(&self) -> Option<ColumnCollection> {
        match self {
            Self::CompositeType(value) if value.type_ref().is_none() => {
                Some(value.get_column_collection())
            }
            Self::ImplicitComposite(value) => Some(value.clone()),
            _ => None,
        }
    }

    pub(crate) fn is_anonymous_record(&self) -> bool {
        self.structural_columns().is_some()
    }

    /// Construct a catalog type with explicit category and collation facts.
    pub fn builtin_with_category(
        full_name: &FullName,
        category: Option<TypeCategory>,
        collatable: Option<bool>,
    ) -> Self {
        Self::Builtin(BuiltinTypeDefinition {
            reference: TypeReference::concrete_type_ref(full_name, false),
            category,
            collatable,
        })
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
    pub const fn owner(&self) -> Option<&OwnerSpecification> {
        match self {
            Self::Enum(definition) => definition.owner(),
            Self::CompositeType(definition) => definition.owner(),
            Self::Domain(definition) => definition.owner(),
            _ => None,
        }
    }
    pub(crate) fn change_owner(&mut self, owner: &OwnerSpecification) -> bool {
        match self {
            Self::Enum(definition) => definition.change_owner(owner),
            Self::CompositeType(definition) => definition.change_owner(owner),
            Self::Domain(definition) => definition.change_owner(owner),
            _ => return false,
        }
        true
    }

    pub(crate) const fn domain_base(&self) -> Option<&TypeReference> {
        if let Self::Domain(domain) = self {
            domain.base_type()
        } else {
            None
        }
    }
    pub(crate) const fn has_structural_ordering(&self) -> bool {
        matches!(
            self,
            Self::Enum(_)
                | Self::CompositeType(_)
                | Self::TableRef(..)
                | Self::ImplicitComposite(_)
        )
    }

    /// type is array?
    #[must_use]
    pub fn is_array(&self) -> Option<bool> {
        match self {
            Self::Array(_) => Some(true),
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

    /// Borrowed declaration reference; anonymous rows and structural arrays have none.
    #[must_use]
    pub const fn type_reference(&self) -> Option<&TypeReference> {
        match self {
            Self::Array(_) => None,
            Self::Enum(definition) => Some(definition.type_ref()),
            Self::Builtin(definition) => Some(&definition.reference),
            Self::Domain(ct) => Some(ct.type_ref()),
            Self::CompositeType(ct) => ct.type_ref().as_ref(),
            Self::TableRef(_, tr) => Some(tr),
            Self::ImplicitComposite(_) => None,
        }
    }

    /// Nominal catalog name; anonymous rows and structural arrays have none.
    #[must_use]
    pub fn full_name(&self) -> Option<&FullName> {
        match self {
            Self::Array(_) => None,
            Self::Enum(create_enum) => Some(create_enum.name()),
            Self::CompositeType(create_composite_type) => create_composite_type.name(),
            Self::Domain(create_domain) => Some(create_domain.name()),
            Self::Builtin(definition) => definition.reference.full_name(),
            Self::TableRef(t, _) => Some(t.full_name()),
            Self::ImplicitComposite(_) => None,
        }
    }

    /// Construct a nominal type with an explicit collation fact and no category.
    #[must_use]
    pub fn builtin(full_name: &FullName, collatable: Option<bool>) -> Self {
        Self::builtin_with_category(full_name, None, collatable)
    }

    /// check type compatibility
    #[must_use]
    pub fn compatible_with(&self, other: &Self) -> bool {
        if matches!(self, Self::Array(_)) || matches!(other, Self::Array(_)) {
            return self == other;
        }
        match (self.full_name(), other.full_name()) {
            (Some(left), Some(right)) => left == right,
            (None, None) => {
                self.matches_structure_with(other, TypeReference::matches_owned_structure)
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod anonymous_compatibility_tests {
    use crate::sem::{ColumnCollection, FullName, PgBuiltInType, TypeDefinition};
    use crate::test_helpers::dynamic_types::record;

    #[rstest::rstest]
    #[case::same("int4", true)]
    #[case::different("text", false)]
    fn optional_names_do_not_establish_structural_compatibility(
        #[case] field: &str,
        #[case] expected: bool,
    ) {
        let left = record(&PgBuiltInType::int4());
        let right = record(&crate::sem::TypeReference::from_master_type_name(field));
        let left = left.type_definition().unwrap();
        let right = right.type_definition().unwrap();
        assert_eq!(left.compatible_with(right), expected);
        assert_eq!(right.compatible_with(left), expected);
        let empty = TypeDefinition::implicit_composite(&ColumnCollection::default());
        assert!(!left.compatible_with(&empty));
        let nominal = TypeDefinition::builtin(&FullName::with_schema("public", "pair"), None);
        assert!(!left.compatible_with(&nominal));
        assert!(!nominal.compatible_with(left));
    }
    #[test]
    fn named_and_anonymous_composites_keep_distinct_identity() {
        use crate::sem::{CompositeFieldCollection, CompositeFieldDefinition, CreateCompositeType};
        let mut fields = CompositeFieldCollection::default();
        fields.push(CompositeFieldDefinition::new(
            "n",
            &Some(PgBuiltInType::int4()),
            &None,
        ));
        let named = TypeDefinition::from(&CreateCompositeType::new(
            &FullName::with_schema("public", "pair"),
            &fields,
        ));
        let anonymous = TypeDefinition::from(&CreateCompositeType::from_fields(&fields));
        assert!(!named.compatible_with(&anonymous));
        assert!(!anonymous.compatible_with(&named));
    }
}

impl TypeDefinition {
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

    pub(crate) fn row_table_name(&self, context: &impl ParseContext) -> Option<TableName> {
        match self {
            Self::TableRef(name, _) => Some(name.clone()),
            Self::Domain(domain) => domain.row_table_name(context),
            _ => None,
        }
    }

    pub(crate) fn row_columns(&self, context: &impl BaseParseContext) -> Option<ColumnCollection> {
        match self {
            Self::TableRef(name, _) => Some(context.get_table(name)?.function_row_columns()),
            Self::Domain(domain) => domain.base_type()?.row_columns(context),
            _ => self.owned_row_columns(),
        }
    }

    /// Row fields carried entirely by owned definitions, without catalog lookup.
    pub(crate) fn owned_row_columns(&self) -> Option<ColumnCollection> {
        match self {
            Self::CompositeType(composite) => Some(composite.get_column_collection()),
            Self::ImplicitComposite(columns) => Some(columns.clone()),
            Self::Domain(domain) => domain.base_type()?.owned_row_columns(),
            _ => None,
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

    pub(crate) fn member_type_from_catalog<TCatalog: BaseParseContext>(
        &self,
        context: &TCatalog,
        member: &str,
    ) -> Option<TypeReference> {
        self.row_columns(context)?
            .get_column_def(&ColumnName::from(member))?
            .get_type_cloned()
    }
}

#[cfg(test)]
mod domain_member_tests {
    use crate::{
        sem::{CreateDomain, CreateEnum, FullName, TypeDefinition, TypeReference},
        test_helpers::{composite_domains, domain_casts},
    };

    #[rstest::rstest]
    #[case::direct("leaf_d", "n", Some("scalar_d"))]
    #[case::nested("leaf_dd", "n", Some("scalar_d"))]
    #[case::domain_field("outer_d", "item", Some("leaf_dd"))]
    #[case::table_row("table_d", "n", Some("scalar_d"))]
    #[case::missing("leaf_dd", "missing", None)]
    #[case::scalar("scalar_d", "n", None)]
    #[case::array("arr_d", "n", None)]
    fn shared_member_lookup_preserves_field_identity(
        #[case] source: &str,
        #[case] member: &str,
        #[case] expected: Option<&str>,
    ) {
        let context = composite_domains::context(false).unwrap();
        let source = composite_domains::reference(source);
        let definition = source.definition_from_catalog(&context).unwrap();
        assert_eq!(
            definition.get_member_type(&context, member),
            expected.map(composite_domains::reference)
        );
    }

    #[test]
    fn owned_domain_layers_resolve_without_a_catalog_name() {
        let context = crate::test_helpers::TestParseContext::default();
        let field = composite_domains::reference("scalar_d");
        let record = crate::test_helpers::dynamic_types::record(&field);
        let inner = TypeReference::dynamic(&TypeDefinition::from(&CreateDomain::new(
            &composite_domains::reference("inner"),
            &Some(record),
        )));
        let outer = TypeDefinition::from(&CreateDomain::new(
            &composite_domains::reference("outer"),
            &Some(inner),
        ));
        assert_eq!(outer.get_member_type(&context, "n"), Some(field));
    }

    #[rstest::rstest]
    fn incomplete_and_cyclic_bases_do_not_resolve_or_fabricate_diagnostics(
        #[values(false, true)] cyclic: bool,
    ) {
        let mut context = domain_casts::context();
        let first = composite_domains::reference("first");
        let second = domain_casts::domain(&mut context, "second", Some(&first));
        domain_casts::domain(&mut context, "first", Some(&second));
        let missing = domain_casts::domain(&mut context, "missing", None);
        let source = if cyclic { first } else { missing };
        assert!(source.row_columns(&context).is_none());
        assert!(source.member_access_problem(&context, "n").is_none());
        assert!(
            source
                .definition_from_catalog(&context)
                .unwrap()
                .get_member_type(&context, "n")
                .is_none()
        );
    }

    #[rstest::rstest]
    fn enum_and_enum_domain_use_the_non_composite_diagnostic(#[values(false, true)] wrapped: bool) {
        let mut context = domain_casts::context();
        let enumeration = TypeReference::dynamic(&TypeDefinition::from(&CreateEnum::new(
            &FullName::with_schema("public", "mood"),
            &["ok".to_owned()],
        )));
        let domain = domain_casts::domain(&mut context, "mood_d", Some(&enumeration));
        let source = if wrapped { domain } else { enumeration };
        assert_eq!(
            source.member_access_problem(&context, "n").unwrap().code(),
            "member_access_requires_composite_type"
        );
        assert!(
            source
                .definition_from_catalog(&context)
                .unwrap()
                .get_member_type(&context, "n")
                .is_none()
        );
    }
}

#[cfg(test)]
mod array_definition_tests {
    use super::{PgBuiltInType, TypeDefinition, TypeReference};

    #[rstest::rstest]
    #[case::same_element("int4", true)]
    #[case::different_element("text", false)]
    fn array_definition_compatibility_retains_element_identity(
        #[case] element: &str,
        #[case] compatible: bool,
    ) {
        let left = TypeDefinition::array(&PgBuiltInType::int4());
        let right = TypeDefinition::array(&TypeReference::from_master_type_name(element));
        assert_eq!(left.compatible_with(&right), compatible);
        assert_eq!(right.compatible_with(&left), compatible);
        let scalar = TypeDefinition::builtin(
            PgBuiltInType::int4()
                .full_name()
                .expect("nominal type identity"),
            None,
        );
        assert!(!left.compatible_with(&scalar));
        assert!(!scalar.compatible_with(&left));
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
