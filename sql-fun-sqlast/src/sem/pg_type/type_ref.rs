use crate::context::BaseParseContext;
use crate::{context::ParseContext, sem::FullName};

use super::{
    TypeDefinition, concrete_type::ConcreteTypeReference, dynamic_type::DynamicTypeReference,
};

/// A concrete named type (optionally an array), or an inferred dynamic type.
///
/// Concrete types display their canonical [`FullName`] followed by `[]` for
/// an array. `FromStr` reads that canonical spelling without SQL case folding
/// or type-alias interpretation; dynamic types have no canonical text form.
/// Use [`crate::sem::parse_client_type`] for SQL client parameter type spelling.
#[derive(Debug, Clone, Eq, PartialEq, Hash, serde::Serialize, serde::Deserialize)]
pub enum TypeReference {
    /// concrete type with array qualifies
    Concrete(ConcreteTypeReference),
    /// dynamic type
    Dynamic(DynamicTypeReference),
}

impl TypeReference {
    pub(crate) fn rename_relation(
        &mut self,
        old: &crate::sem::TableName,
        new: &crate::sem::TableName,
    ) {
        if let Self::Dynamic(value) = self {
            value.rename_relation(old, new);
        }
        if let Self::Concrete(value) = self
            && value.full_name() == old.full_name()
        {
            *self = Self::concrete_type_ref(new.full_name(), value.is_array());
        }
    }

    /// create a reference for dynamic type
    #[must_use]
    pub fn dynamic(type_def: &TypeDefinition) -> Self {
        Self::Dynamic(DynamicTypeReference::new(type_def))
    }

    /// type full name
    #[track_caller]
    #[must_use]
    pub fn full_name(&self) -> &FullName {
        match self {
            Self::Concrete(c) => c.full_name(),
            Self::Dynamic(_) => todo!(),
        }
    }

    /// Construct an array retaining this element's nominal or structural identity.
    /// PostgreSQL dimensions do not introduce additional array type identities.
    #[must_use]
    pub fn array_type(&self) -> Self {
        if self.is_array() == Some(true) {
            return self.clone();
        }
        match self {
            Self::Concrete(value) => Self::concrete_type_ref(value.full_name(), true),
            Self::Dynamic(_) => Self::dynamic(&TypeDefinition::array(self)),
        }
    }

    /// get type definition when owned
    #[must_use]
    pub fn type_definition(&self) -> Option<&TypeDefinition> {
        match self {
            Self::Concrete(_c) => None,
            Self::Dynamic(d) => Some(d.type_definition()),
        }
    }

    pub(crate) fn definition_from_catalog<'a>(
        &'a self,
        context: &'a impl BaseParseContext,
    ) -> Option<&'a TypeDefinition> {
        if let Some(definition) = self.type_definition() {
            Some(definition)
        } else {
            context.get_type(self.full_name())
        }
    }

    pub(crate) fn row_columns<P: ParseContext>(
        &self,
        context: &P,
    ) -> Option<crate::sem::ColumnCollection> {
        self.definition_from_catalog(context)?.row_columns(context)
    }

    pub(crate) fn implicit_match_context(
        &self,
        target: &Self,
        context: &impl BaseParseContext,
    ) -> Option<crate::sem::CastContext> {
        if self.is_dynamic() || target.is_dynamic() {
            let accepts_record = *target == Self::from_master_type_name("record")
                && self
                    .type_definition()
                    .is_some_and(|definition| definition.structural_columns().is_some());
            if accepts_record || self.matches_structure(target, context) {
                Some(crate::sem::CastContext::no_conversion())
            } else {
                None
            }
        } else {
            context
                .get_implicit_cast(self, target)
                .map(|cast| cast.context())
        }
    }

    /// Structural evidence available without a borrowed catalog. Named types
    /// still require identity; anonymous records compare positional field types.
    pub(crate) fn implicit_match_owned(&self, target: &Self) -> bool {
        if *target == Self::from_master_type_name("record")
            && self
                .type_definition()
                .is_some_and(TypeDefinition::is_anonymous_record)
        {
            return true;
        }
        self.matches_owned_structure(target)
    }

    fn matches_owned_structure(&self, other: &Self) -> bool {
        if self == other {
            return true;
        }
        if !self.is_dynamic() && !other.is_dynamic() {
            return false;
        }
        if self.is_array() == Some(true) || other.is_array() == Some(true) {
            return self.is_array() == Some(true)
                && other.is_array() == Some(true)
                && self
                    .element_type()
                    .matches_owned_structure(&other.element_type());
        }
        self.type_definition()
            .zip(other.type_definition())
            .is_some_and(|(left, right)| {
                left.matches_structure_with(right, Self::matches_owned_structure)
            })
    }

    // Structural matching is separate from catalog cast lookup: anonymous
    // definitions cannot be used as keys in the cast master.
    pub(crate) fn matches_structure(&self, other: &Self, context: &impl BaseParseContext) -> bool {
        if self == other {
            return true;
        }
        if !self.is_dynamic() && !other.is_dynamic() {
            return false;
        }
        if self.is_array() == Some(true) || other.is_array() == Some(true) {
            return self.is_array() == Some(true)
                && other.is_array() == Some(true)
                && self
                    .element_type()
                    .matches_structure(&other.element_type(), context);
        }
        let left = self.definition_from_catalog(context);
        let right = other.definition_from_catalog(context);
        match (left, right) {
            (Some(left), Some(right)) => left.matches_structure(right, context),
            _ => false,
        }
    }

    /// type is array?
    #[must_use]
    pub fn is_array(&self) -> Option<bool> {
        match self {
            Self::Concrete(c) => Some(c.is_array()),
            Self::Dynamic(d) => d.is_array(),
        }
    }

    /// get element type reference
    ///
    /// # Panics
    ///
    /// self requires array type
    ///
    #[must_use]
    pub fn element_type(&self) -> Self {
        match self {
            Self::Concrete(c) => c.element_type(),
            Self::Dynamic(value) => value
                .type_definition()
                .array_element()
                .expect("dynamic array definition")
                .clone(),
        }
    }

    /// is record type?
    #[must_use]
    pub fn is_record(&self) -> bool {
        match self {
            Self::Concrete(c) => c.is_record(),
            Self::Dynamic(value) => value.is_array() != Some(true),
        }
    }

    /// create a type reference to named type
    #[must_use]
    pub fn from_full_name(full_name: FullName) -> Self {
        Self::Concrete(ConcreteTypeReference::new(&full_name, false))
    }

    /// coverts type name with '_' as array
    #[must_use]
    pub fn from_master_type_name(name: &str) -> Self {
        Self::Concrete(ConcreteTypeReference::from_master_type_name(name))
    }

    /// create concrete type reference
    #[must_use]
    pub fn concrete_type_ref(full_name: &FullName, is_array: bool) -> Self {
        Self::Concrete(ConcreteTypeReference::new(full_name, is_array))
    }

    /// is multi range type?
    #[must_use]
    pub fn is_range(&self) -> bool {
        match self {
            Self::Concrete(c) => c.is_range(),
            Self::Dynamic(_d) => todo!(),
        }
    }

    /// get subtype from range type
    #[must_use]
    pub fn range_subtype(&self) -> Self {
        match self {
            Self::Concrete(c) => c.range_subtype(),
            Self::Dynamic(_d) => todo!(),
        }
    }

    /// is multi range type?
    #[must_use]
    pub fn is_multi_range(&self) -> bool {
        match self {
            Self::Concrete(c) => c.is_multi_range(),
            Self::Dynamic(_d) => todo!(),
        }
    }

    /// get base element type from multi range type
    #[must_use]
    pub fn multi_range_base_element(&self) -> Self {
        match self {
            Self::Concrete(c) => c.multi_range_base_element(),
            Self::Dynamic(_d) => todo!(),
        }
    }

    /// type is simple inference arg
    #[must_use]
    pub fn is_simple_infer_arg(&self) -> bool {
        match self {
            Self::Concrete(c) => c.is_simple_infer_arg(),
            Self::Dynamic(_d) => todo!(),
        }
    }

    /// type is common inference arg
    #[must_use]
    pub fn is_common_infer_arg(&self) -> bool {
        match self {
            Self::Concrete(c) => c.is_common_infer_arg(),
            Self::Dynamic(_d) => todo!(),
        }
    }

    /// type is just `anyelement`
    #[must_use]
    pub fn is_any_element(&self) -> bool {
        match self {
            Self::Concrete(c) => c.is_any_element(),
            Self::Dynamic(_d) => todo!(),
        }
    }

    /// type is just `anyarrry`
    #[must_use]
    pub fn is_any_array(&self) -> bool {
        match self {
            Self::Concrete(c) => c.is_any_array(),
            Self::Dynamic(_d) => todo!(),
        }
    }

    /// type is just `anynonarray`
    #[must_use]
    pub fn is_any_non_array(&self) -> bool {
        match self {
            Self::Concrete(c) => c.is_any_non_array(),
            Self::Dynamic(_d) => todo!(),
        }
    }

    /// type is just `anyrange`
    #[must_use]
    pub fn is_any_range(&self) -> bool {
        match self {
            Self::Concrete(c) => c.is_any_range(),
            Self::Dynamic(_d) => todo!(),
        }
    }

    /// type is just `anymultirange`
    #[must_use]
    pub fn is_any_multi_range(&self) -> bool {
        match self {
            Self::Concrete(c) => c.is_any_multi_range(),
            Self::Dynamic(_d) => todo!(),
        }
    }

    /// type is just `anycompatible`
    #[must_use]
    pub fn is_any_compatible(&self) -> bool {
        match self {
            Self::Concrete(c) => c.is_any_compatible(),
            Self::Dynamic(_d) => todo!(),
        }
    }

    /// type is just `anycompatiblearray`
    #[must_use]
    pub fn is_any_compatible_array(&self) -> bool {
        match self {
            Self::Concrete(c) => c.is_any_compatible_array(),
            Self::Dynamic(_d) => todo!(),
        }
    }

    /// type is just `anycompatiblenonarray`
    #[must_use]
    pub fn is_any_compatible_non_array(&self) -> bool {
        match self {
            Self::Concrete(c) => c.is_any_compatible_non_array(),
            Self::Dynamic(_d) => todo!(),
        }
    }

    /// type is just `anycompatiblerange`
    #[must_use]
    pub fn is_any_compatible_range(&self) -> bool {
        match self {
            Self::Concrete(c) => c.is_any_compatible_range(),
            Self::Dynamic(_d) => todo!(),
        }
    }

    /// type is just `anycompatiblemultirange`
    #[must_use]
    pub fn is_any_compatible_multi_range(&self) -> bool {
        match self {
            Self::Concrete(c) => c.is_any_compatible_multi_range(),
            Self::Dynamic(_d) => todo!(),
        }
    }

    /// convert to range type
    #[must_use]
    pub fn into_range_type(&self) -> Self {
        match self {
            Self::Concrete(c) => c.get_range_type(),
            Self::Dynamic(_d) => todo!(),
        }
    }

    /// convert to multi range type
    #[must_use]
    pub fn into_multi_range_type(&self) -> Self {
        match self {
            Self::Concrete(c) => c.get_multi_range_type(),
            Self::Dynamic(_d) => todo!(),
        }
    }

    /// apply simple inference result
    pub fn apply_simple_infer_result(&mut self, infer_result_type: &Self) {
        match self {
            Self::Concrete(c) => {
                if infer_result_type.is_dynamic() && c.is_any_array() {
                    *self = infer_result_type.array_type();
                } else if infer_result_type.is_dynamic()
                    && (c.is_any_element() || c.is_any_non_array())
                {
                    *self = infer_result_type.clone();
                } else {
                    c.apply_simple_infer_result(infer_result_type);
                }
            }
            Self::Dynamic(_d) => todo!(),
        }
    }

    /// apply common inference result
    pub fn apply_common_infer_result(&mut self, infer_result_type: &Self) {
        match self {
            Self::Concrete(c) => {
                if infer_result_type.is_dynamic() && c.is_any_compatible_array() {
                    *self = infer_result_type.array_type();
                } else if infer_result_type.is_dynamic()
                    && (c.is_any_compatible() || c.is_any_compatible_non_array())
                {
                    *self = infer_result_type.clone();
                } else {
                    c.apply_common_infer_result(infer_result_type);
                }
            }
            Self::Dynamic(_d) => todo!(),
        }
    }

    /// type is dynamic
    #[must_use]
    pub const fn is_dynamic(&self) -> bool {
        matches!(self, Self::Dynamic(_))
    }
}

#[cfg(test)]
mod array_identity_tests {
    use super::TypeReference;
    use crate::sem::create_comp_type::{CompositeFieldCollection, CompositeFieldDefinition};
    use crate::sem::{CreateCompositeType, PgBuiltInType, TypeDefinition};

    fn record(field_type: &TypeReference) -> TypeReference {
        let mut fields = CompositeFieldCollection::default();
        fields.push(CompositeFieldDefinition::new(
            "n",
            &Some(field_type.clone()),
            &None,
        ));
        TypeReference::dynamic(&TypeDefinition::from(&CreateCompositeType::from_fields(
            &fields,
        )))
    }

    #[test]
    fn anonymous_record_accepts_generic_record_without_cast_master() {
        let mut context = crate::test_helpers::TestParseContext::default();
        let source = record(&PgBuiltInType::int4());
        let target = TypeReference::from_master_type_name("record");
        context.set_get_implicit_cast_result(&source, &target, None);
        assert!(source.implicit_match_context(&target, &context).is_some());
        assert!(!source.matches_structure(&target, &context));
    }

    #[rstest::rstest]
    fn dynamic_matching_uses_structure_instead_of_cast_master(
        #[values(false, true)] arrays: bool,
        #[values(false, true)] dynamic_target: bool,
        #[values(false, true)] reverse: bool,
        #[values(false, true)] matching: bool,
    ) {
        use crate::sem::{CastContext, CastDefinition, FullName};
        use crate::test_helpers::TestParseContext;

        let mut context = TestParseContext::default();
        let source = record(&PgBuiltInType::int4());
        let field_type = if matching {
            PgBuiltInType::int4()
        } else {
            PgBuiltInType::text()
        };
        let mut fields = CompositeFieldCollection::default();
        fields.push(CompositeFieldDefinition::new(
            "different_name",
            &Some(field_type),
            &None,
        ));
        let target = if dynamic_target {
            TypeReference::dynamic(&TypeDefinition::from(&CreateCompositeType::from_fields(
                &fields,
            )))
        } else {
            let name = FullName::with_schema("public", "row_type");
            let definition = TypeDefinition::from(&CreateCompositeType::new(&name, &fields));
            context.set_get_type_impl_result(&name, &definition);
            TypeReference::from_full_name(name)
        };
        let (source, target) = if arrays {
            (source.array_type(), target.array_type())
        } else {
            (source, target)
        };
        let (source, target) = if reverse {
            (target, source)
        } else {
            (source, target)
        };
        let expected = matching && dynamic_target;
        // A deliberately contradictory catalog answer proves that Dynamic
        // operands are evaluated without consulting the cast master.
        context.set_get_implicit_cast_result(
            &source,
            &target,
            if expected {
                None
            } else {
                Some(CastDefinition::new(CastContext::implicit()))
            },
        );
        assert_eq!(
            source.implicit_match_context(&target, &context).is_some(),
            expected
        );
    }

    #[rstest::rstest]
    #[case::same_definition("row_type", "int4", true)]
    #[case::different_name("other_type", "int4", false)]
    #[case::different_field("row_type", "text", false)]
    fn loaded_nominal_types_compare_definitions_without_cast_lookup(
        #[case] target_name: &str,
        #[case] field_type: &str,
        #[case] expected: bool,
        #[values(false, true)] arrays: bool,
        #[values(false, true)] reverse: bool,
    ) {
        use crate::sem::{CastContext, CastDefinition, FullName};
        use crate::test_helpers::TestParseContext;

        let mut context = TestParseContext::default();
        let name = FullName::with_schema("public", "row_type");
        let mut fields = CompositeFieldCollection::default();
        fields.push(CompositeFieldDefinition::new(
            "n",
            &Some(PgBuiltInType::int4()),
            &None,
        ));
        let source = TypeReference::dynamic(&TypeDefinition::from(&CreateCompositeType::new(
            &name, &fields,
        )));
        let target_name = FullName::with_schema("public", target_name);
        let mut fields = CompositeFieldCollection::default();
        fields.push(CompositeFieldDefinition::new(
            "n",
            &Some(TypeReference::from_master_type_name(field_type)),
            &None,
        ));
        context.set_get_type_impl_result(
            &target_name,
            &TypeDefinition::from(&CreateCompositeType::new(&target_name, &fields)),
        );
        let target = TypeReference::from_full_name(target_name);
        let (source, target) = if arrays {
            (source.array_type(), target.array_type())
        } else {
            (source, target)
        };
        let (source, target) = if reverse {
            (target, source)
        } else {
            (source, target)
        };
        context.set_get_implicit_cast_result(
            &source,
            &target,
            if expected {
                None
            } else {
                Some(CastDefinition::new(CastContext::implicit()))
            },
        );
        assert_eq!(
            source.implicit_match_context(&target, &context).is_some(),
            expected
        );
    }

    #[test]
    fn dynamic_array_preserves_element_identity_and_serialization() -> testresult::TestResult {
        let element = record(&PgBuiltInType::int4());
        let array = element.array_type();
        assert!(array.is_dynamic());
        assert_eq!(array.is_array(), Some(true));
        assert!(!array.is_record());
        assert_eq!(array.element_type(), element);
        assert_eq!(array.array_type(), array);
        assert_ne!(array, record(&PgBuiltInType::text()).array_type());
        let encoded = ::serde_json::to_string(&array)?;
        let decoded: TypeReference = ::serde_json::from_str(&encoded)?;
        assert_eq!(decoded, array);
        assert_eq!(decoded.element_type(), element);
        Ok(())
    }

    #[rstest::rstest]
    #[case::element("anyelement", false, false)]
    #[case::array("anyarray", false, true)]
    #[case::nonarray("anynonarray", false, false)]
    #[case::compatible("anycompatible", true, false)]
    #[case::compatible_array("anycompatiblearray", true, true)]
    #[case::compatible_nonarray("anycompatiblenonarray", true, false)]
    fn substitution_keeps_anonymous_type_components(
        #[case] name: &str,
        #[case] common: bool,
        #[case] array: bool,
    ) {
        let element = record(&PgBuiltInType::int4());
        let mut placeholder = TypeReference::from_master_type_name(name);
        if common {
            placeholder.apply_common_infer_result(&element);
        } else {
            placeholder.apply_simple_infer_result(&element);
        }
        let expected = if array { element.array_type() } else { element };
        assert_eq!(placeholder, expected);
    }

    #[test]
    fn named_arrays_keep_the_existing_serialized_representation() -> testresult::TestResult {
        let array = PgBuiltInType::int4().array_type();
        assert_eq!(array, TypeReference::from_master_type_name("_int4"));
        assert_eq!(
            ::serde_json::to_value(&array)?,
            ::serde_json::to_value(TypeReference::from_master_type_name("_int4"))?
        );
        assert_eq!(array.element_type(), PgBuiltInType::int4());
        Ok(())
    }
}

impl std::fmt::Display for TypeReference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Concrete(c) => write!(f, "{c}"),
            Self::Dynamic(_d) => todo!(),
        }
    }
}

impl std::str::FromStr for TypeReference {
    type Err = crate::sem::AnalysisError;

    /// Read canonical concrete type spelling, preserving quoted suffixes.
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let (name, is_array) = if let Some(name) = value.strip_suffix("[]") {
            (name, true)
        } else {
            (value, false)
        };
        Ok(Self::concrete_type_ref(
            &name.parse::<FullName>()?,
            is_array,
        ))
    }
}

#[cfg(test)]
mod canonical_type_tests {
    use super::TypeReference;
    use crate::sem::FullName;

    #[rstest::rstest]
    #[case::plain(FullName::with_schema("fixture", "item"))]
    #[case::dot(FullName::with_schema("a.b", "item"))]
    #[case::quote(FullName::with_schema("a", "b\"c"))]
    #[case::literal_array_suffix(FullName::with_schema("a", "item[]"))]
    #[case::catalog(FullName::with_catalog("db", "a.b", "c"))]
    fn preserves_type_identity(
        #[case] name: FullName,
        #[values(false, true)] array: bool,
    ) -> testresult::TestResult {
        let reference = TypeReference::concrete_type_ref(&name, array);
        assert_eq!(reference.to_string().parse::<TypeReference>()?, reference);
        assert_ne!(
            reference.to_string(),
            TypeReference::concrete_type_ref(&name, !array).to_string()
        );
        Ok(())
    }

    #[test]
    fn scalar_array_spelling_is_not_an_array() {
        let scalar = TypeReference::concrete_type_ref(&FullName::with_schema("a", "item[]"), false);
        let array = TypeReference::concrete_type_ref(&FullName::with_schema("a", "item"), true);
        assert_ne!(scalar.to_string(), array.to_string());
    }
}
