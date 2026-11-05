use crate::{
    ast::{Dialect, TypeName},
    context::{BaseParseContext, ParseContext},
    names::{FullName, TableName},
    sem::{
        AnalysisError, AnalysisProblem, CastContext as SemCastContext,
        ColumnCollection as SemColumnCollection,
        pg_type::{
            TypeDefinition, concrete_type::ConcreteTypeReference,
            dynamic_type::DynamicTypeReference,
        },
    },
};
use std::collections::HashSet;

/// A concrete named type (optionally an array), or an inferred dynamic type.
///
/// Concrete types display their canonical [`FullName`] followed by `[]` for
/// an array. Anonymous display is diagnostic structural text, not a catalog name.
/// `FromStr` reads that canonical spelling without SQL case folding
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
    /// PostgreSQL collation capability, following existing domain/array references.
    ///
    /// `None` means an undefined fact or unresolved reference (also returned for
    /// other dialects). This query does not validate SQL, report diagnostics or
    /// discover supplemental metadata. Nominal vectors use their own fact.
    pub fn collation_capability(&self, context: &impl ParseContext) -> Option<bool> {
        if context.dialect() != Dialect::postgres() {
            return None;
        }
        self.collation_capability_in(context, &mut HashSet::new())
    }
}

#[cfg(test)]
mod collation_capability_tests {
    #[cfg(feature = "sqlite")]
    use crate::ast::Dialect;
    use crate::{
        names::FullName,
        sem::{CreateDomain, TypeDefinition, TypeReference},
        test_helpers::TestParseContext,
    };

    #[rstest::rstest]
    fn retained_fact_is_used_without_name_or_element_inference(
        #[values(Some(true), Some(false), None)] fact: Option<bool>,
        #[values(false, true)] dynamic: bool,
        #[values(false, true)] array: bool,
    ) {
        let mut context = TestParseContext::default();
        // A qualified, vector-like name must not determine the answer.
        let name = FullName::with_schema("extension schema", "_custom_vector");
        let definition = TypeDefinition::builtin(&name, fact);
        context.set_get_type_impl_result(&name, &definition);
        let reference = if dynamic {
            TypeReference::dynamic(&definition)
        } else {
            TypeReference::concrete_type_ref(&name, false)
        };
        let reference = if array {
            reference.array_type()
        } else {
            reference
        };
        assert_eq!(reference.collation_capability(&context), fact);
        assert_eq!(context.reported_problem_count(), 0);
    }

    #[rstest::rstest]
    fn follows_nested_domains_and_arrays(
        #[values(Some(true), Some(false), None)] fact: Option<bool>,
        #[values(false, true)] array_base: bool,
        #[values(false, true)] array_domain: bool,
        #[values(false, true)] dynamic: bool,
    ) {
        let mut context = TestParseContext::default();
        let base_name = FullName::with_schema("extension", "base");
        let base = TypeReference::concrete_type_ref(&base_name, array_base);
        context.set_get_type_impl_result(&base_name, &TypeDefinition::builtin(&base_name, fact));
        let first =
            TypeReference::concrete_type_ref(&FullName::with_schema("public", "first"), false);
        let second =
            TypeReference::concrete_type_ref(&FullName::with_schema("public", "second"), false);
        let first_definition = TypeDefinition::from(&CreateDomain::new(&first, &Some(base)));
        context.set_get_type_impl_result(
            first.full_name().expect("nominal type identity"),
            &first_definition,
        );
        let second_definition = TypeDefinition::from(&CreateDomain::new(&second, &Some(first)));
        context.set_get_type_impl_result(
            second.full_name().expect("nominal type identity"),
            &second_definition,
        );
        let reference = if dynamic {
            TypeReference::dynamic(&second_definition)
        } else {
            second
        };
        let reference = if array_domain {
            reference.array_type()
        } else {
            reference
        };
        assert_eq!(reference.collation_capability(&context), fact);
    }

    #[rstest::rstest]
    #[case::missing_base(None)]
    #[case::missing_definition(Some("missing"))]
    #[case::cycle(Some("cycle"))]
    fn unresolved_and_cyclic_domains_have_no_fact(#[case] base: Option<&str>) {
        let mut context = TestParseContext::default();
        let reference =
            TypeReference::concrete_type_ref(&FullName::with_schema("public", "cycle"), false);
        let base = base.map(|name| {
            TypeReference::concrete_type_ref(&FullName::with_schema("public", name), false)
        });
        let domain = TypeDefinition::from(&CreateDomain::new(&reference, &base));
        context.set_get_type_impl_result(
            reference.full_name().expect("nominal type identity"),
            &domain,
        );
        assert_eq!(reference.collation_capability(&context), None);
        assert_eq!(
            TypeReference::dynamic(&domain).collation_capability(&context),
            None
        );
    }

    #[test]
    #[cfg(feature = "sqlite")]
    fn other_dialects_do_not_expose_postgres_facts() {
        let context = TestParseContext::default().with_dialect(Dialect::sqlite());
        let reference = TypeReference::dynamic(&TypeDefinition::builtin(
            &FullName::with_schema("pg_catalog", "text"),
            Some(true),
        ));
        assert_eq!(reference.collation_capability(&context), None);
    }
}

impl TypeReference {
    pub(super) fn collation_capability_in(
        &self,
        context: &impl ParseContext,
        visited: &mut HashSet<FullName>,
    ) -> Option<bool> {
        match self {
            Self::Concrete(reference) => {
                if reference.is_array() {
                    return reference
                        .element_type()
                        .collation_capability_in(context, visited);
                }
                if !visited.insert(reference.full_name().clone()) {
                    return None;
                }
                self.definition_from_catalog(context)?
                    .collation_capability_in(context, visited)
            }
            Self::Dynamic(reference) => reference
                .type_definition()
                .collation_capability_in(context, visited),
        }
    }
}

impl TypeReference {
    pub(crate) fn rename_relation(&mut self, old: &TableName, new: &TableName) {
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
}

#[cfg(test)]
mod test_dynamic_identity {
    use crate::{
        names::FullName,
        sem::{PgBuiltInType, TypeDefinition, TypeReference},
        test_helpers::{TestParseContext, dynamic_types::record},
    };

    #[rstest::rstest]
    fn anonymous_identity_retains_definition_and_elements(#[values(false, true)] array: bool) {
        let row = record(&PgBuiltInType::int4());
        let reference = if array { row.array_type() } else { row.clone() };
        assert_eq!(reference.full_name(), None);
        assert_eq!(reference.is_array(), Some(array));
        assert_eq!(reference.is_record(), !array);
        assert_eq!(reference.type_definition().unwrap().type_reference(), None);
        let restored: TypeReference =
            ::serde_json::from_str(&::serde_json::to_string(&reference).unwrap()).unwrap();
        assert_eq!(restored, reference);
        if array {
            assert_eq!(reference.element_type(), row);
        } else {
            let context = TestParseContext::default();
            let columns = reference.row_columns(&context).unwrap();
            assert_eq!(
                columns.column_at(0).unwrap().get_type(),
                Some(&PgBuiltInType::int4())
            );
            let implicit = TypeDefinition::implicit_composite(&columns);
            assert_eq!(implicit.full_name(), None);
            assert_eq!(implicit.type_reference(), None);
            assert!(reference.matches_structure(&TypeReference::dynamic(&implicit), &context));
        }
    }

    #[rstest::rstest]
    #[case::row(false)]
    #[case::array(true)]
    fn anonymous_types_are_neither_ranges_nor_placeholders(#[case] array: bool) {
        let row = record(&PgBuiltInType::int4());
        let reference = if array { row.array_type() } else { row };
        assert!(!reference.is_range());
        assert!(!reference.is_multi_range());
        assert!(!reference.is_simple_infer_arg());
        assert!(!reference.is_common_infer_arg());
        assert!(!reference.is_any_element());
        assert!(!reference.is_any_array());
        assert!(!reference.is_any_non_array());
        assert!(!reference.is_any_range());
        assert!(!reference.is_any_multi_range());
        assert!(!reference.is_any_compatible());
        assert!(!reference.is_any_compatible_array());
        assert!(!reference.is_any_compatible_non_array());
        assert!(!reference.is_any_compatible_range());
        assert!(!reference.is_any_compatible_multi_range());
        let mut fixed = reference.clone();
        fixed.apply_simple_infer_result(&PgBuiltInType::text());
        fixed.apply_common_infer_result(&PgBuiltInType::text());
        assert_eq!(fixed, reference);
    }

    #[rstest::rstest]
    #[case::scalar("int4", false, false, false)]
    #[case::record("record", true, false, false)]
    #[case::range("int4range", false, true, false)]
    #[case::multirange("int4multirange", false, false, true)]
    fn owned_nominal_classification_uses_its_definition(
        #[case] name: &str,
        #[case] record: bool,
        #[case] range: bool,
        #[case] multirange: bool,
    ) {
        let name = FullName::with_schema("pg_catalog", name);
        let reference = TypeReference::dynamic(&TypeDefinition::builtin(&name, None));
        assert_eq!(reference.full_name(), Some(&name));
        assert_eq!(reference.to_string(), name.to_string());
        assert_eq!(reference.is_record(), record);
        assert_eq!(reference.is_range(), range);
        assert_eq!(reference.is_multi_range(), multirange);
    }
}

impl TypeReference {
    /// Nominal identity, or `None` for an anonymous row or structural array.
    /// Owned named definitions retain their actual catalog name.
    #[must_use]
    pub fn full_name(&self) -> Option<&FullName> {
        match self {
            Self::Concrete(c) => Some(c.full_name()),
            Self::Dynamic(d) => d.type_definition().full_name(),
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
}

#[cfg(test)]
mod test_array_identity {

    use crate::{
        sem::{PgBuiltInType, pg_type::type_ref::TypeReference},
        test_helpers::dynamic_types::record,
    };

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

impl TypeReference {
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
            context.get_type(self.full_name()?)
        }
    }

    pub(crate) fn row_columns(
        &self,
        context: &impl BaseParseContext,
    ) -> Option<SemColumnCollection> {
        let base = self.member_base(context)?;
        if base.is_array() == Some(true) {
            return None;
        }
        base.definition_from_catalog(context)?.row_columns(context)
    }

    pub(crate) fn owned_row_columns(&self) -> Option<SemColumnCollection> {
        if self.is_array() == Some(true) {
            return None;
        }
        self.type_definition()?.owned_row_columns()
    }

    fn member_base<'a>(&'a self, context: &'a impl BaseParseContext) -> Option<&'a Self> {
        let mut base = self;
        let mut visited = std::collections::HashSet::new();
        loop {
            if base.is_array() == Some(true) {
                return Some(base);
            }
            if !visited.insert(base.clone()) {
                return None;
            }
            let definition = base.definition_from_catalog(context)?;
            if !definition.is_domain() {
                return Some(base);
            }
            base = definition.domain_base()?;
        }
    }

    /// Classify an invalid member access without replacing the original type.
    pub(crate) fn member_access_problem(
        &self,
        context: &impl ParseContext,
        member: &str,
    ) -> Option<AnalysisProblem> {
        let base = self.member_base(context)?;
        if self.row_columns(context).is_some()
            || (base.is_array() != Some(true) && base.is_record())
        {
            Some(AnalysisProblem::composite_type_field_not_found(
                self, member,
            ))
        } else {
            Some(AnalysisProblem::member_access_requires_composite_type(
                sql_fun_core::analysis_problem::FullName::new(&self.to_string()),
                member,
            ))
        }
    }

    pub(crate) fn assignment_match_context(
        &self,
        target: &Self,
        context: &impl BaseParseContext,
    ) -> Option<SemCastContext> {
        if self.is_dynamic() || target.is_dynamic() {
            self.implicit_match_context(target, context)
        } else {
            context
                .get_assignment_cast(self, target)
                .map(|cast| cast.context())
        }
    }

    pub(crate) fn implicit_match_context(
        &self,
        target: &Self,
        context: &impl BaseParseContext,
    ) -> Option<SemCastContext> {
        if self.is_dynamic() || target.is_dynamic() {
            let accepts_record = *target == Self::from_master_type_name("record")
                && self
                    .type_definition()
                    .is_some_and(|definition| definition.structural_columns().is_some());
            if accepts_record || self.matches_structure(target, context) {
                Some(SemCastContext::no_conversion())
            } else {
                None
            }
        } else {
            context
                .get_implicit_cast(self, target)
                .map(|cast| cast.context())
        }
    }
}

#[cfg(test)]
mod test_dynamic_matching {
    use crate::{
        sem::{
            CreateCompositeType, PgBuiltInType, TypeDefinition,
            create_comp_type::{CompositeFieldCollection, CompositeFieldDefinition},
            pg_type::type_ref::TypeReference,
        },
        test_helpers::{TestParseContext, dynamic_types::record},
    };

    #[test]
    fn anonymous_record_accepts_generic_record_without_cast_master() {
        let mut context = TestParseContext::default();
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
        use crate::{
            names::FullName,
            sem::{CastContext, CastDefinition},
            test_helpers::TestParseContext,
        };

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
        use crate::{
            names::FullName,
            sem::{CastContext, CastDefinition},
            test_helpers::TestParseContext,
        };

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
}

impl TypeReference {
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

    pub(super) fn matches_owned_structure(&self, other: &Self) -> bool {
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
            Self::Dynamic(value) => {
                value.type_definition().is_anonymous_record()
                    || value
                        .type_definition()
                        .type_reference()
                        .is_some_and(Self::is_record)
            }
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
            Self::Dynamic(d) => d
                .type_definition()
                .type_reference()
                .is_some_and(Self::is_range),
        }
    }

    /// Get the subtype of a range. Nominal subtype metadata remains pending #914.
    ///
    /// # Panics
    /// Requires a range receiver; existing nominal metadata extraction is unimplemented.
    #[must_use]
    pub fn range_subtype(&self) -> Self {
        match self {
            Self::Concrete(c) => c.range_subtype(),
            Self::Dynamic(d) => {
                assert!(self.is_range(), "requires a range reference");
                d.type_definition()
                    .type_reference()
                    .expect("named range definition")
                    .range_subtype()
            }
        }
    }

    /// is multi range type?
    #[must_use]
    pub fn is_multi_range(&self) -> bool {
        match self {
            Self::Concrete(c) => c.is_multi_range(),
            Self::Dynamic(d) => d
                .type_definition()
                .type_reference()
                .is_some_and(Self::is_multi_range),
        }
    }

    /// Get the subtype of a multirange. Nominal subtype metadata remains pending #914.
    ///
    /// # Panics
    /// Requires a multirange receiver; existing nominal metadata extraction is unimplemented.
    #[must_use]
    pub fn multi_range_base_element(&self) -> Self {
        match self {
            Self::Concrete(c) => c.multi_range_base_element(),
            Self::Dynamic(d) => {
                assert!(self.is_multi_range(), "requires a multirange reference");
                d.type_definition()
                    .type_reference()
                    .expect("named range definition")
                    .multi_range_base_element()
            }
        }
    }

    /// type is simple inference arg
    #[must_use]
    pub fn is_simple_infer_arg(&self) -> bool {
        match self {
            Self::Concrete(c) => c.is_simple_infer_arg(),
            Self::Dynamic(d) => d
                .type_definition()
                .type_reference()
                .is_some_and(Self::is_simple_infer_arg),
        }
    }

    /// type is common inference arg
    #[must_use]
    pub fn is_common_infer_arg(&self) -> bool {
        match self {
            Self::Concrete(c) => c.is_common_infer_arg(),
            Self::Dynamic(d) => d
                .type_definition()
                .type_reference()
                .is_some_and(Self::is_common_infer_arg),
        }
    }

    /// type is just `anyelement`
    #[must_use]
    pub fn is_any_element(&self) -> bool {
        match self {
            Self::Concrete(c) => c.is_any_element(),
            Self::Dynamic(d) => d
                .type_definition()
                .type_reference()
                .is_some_and(Self::is_any_element),
        }
    }

    /// type is just `anyarrry`
    #[must_use]
    pub fn is_any_array(&self) -> bool {
        match self {
            Self::Concrete(c) => c.is_any_array(),
            Self::Dynamic(d) => d
                .type_definition()
                .type_reference()
                .is_some_and(Self::is_any_array),
        }
    }

    /// type is just `anynonarray`
    #[must_use]
    pub fn is_any_non_array(&self) -> bool {
        match self {
            Self::Concrete(c) => c.is_any_non_array(),
            Self::Dynamic(d) => d
                .type_definition()
                .type_reference()
                .is_some_and(Self::is_any_non_array),
        }
    }

    /// type is just `anyrange`
    #[must_use]
    pub fn is_any_range(&self) -> bool {
        match self {
            Self::Concrete(c) => c.is_any_range(),
            Self::Dynamic(d) => d
                .type_definition()
                .type_reference()
                .is_some_and(Self::is_any_range),
        }
    }

    /// type is just `anymultirange`
    #[must_use]
    pub fn is_any_multi_range(&self) -> bool {
        match self {
            Self::Concrete(c) => c.is_any_multi_range(),
            Self::Dynamic(d) => d
                .type_definition()
                .type_reference()
                .is_some_and(Self::is_any_multi_range),
        }
    }

    /// type is just `anycompatible`
    #[must_use]
    pub fn is_any_compatible(&self) -> bool {
        match self {
            Self::Concrete(c) => c.is_any_compatible(),
            Self::Dynamic(d) => d
                .type_definition()
                .type_reference()
                .is_some_and(Self::is_any_compatible),
        }
    }

    /// type is just `anycompatiblearray`
    #[must_use]
    pub fn is_any_compatible_array(&self) -> bool {
        match self {
            Self::Concrete(c) => c.is_any_compatible_array(),
            Self::Dynamic(d) => d
                .type_definition()
                .type_reference()
                .is_some_and(Self::is_any_compatible_array),
        }
    }

    /// type is just `anycompatiblenonarray`
    #[must_use]
    pub fn is_any_compatible_non_array(&self) -> bool {
        match self {
            Self::Concrete(c) => c.is_any_compatible_non_array(),
            Self::Dynamic(d) => d
                .type_definition()
                .type_reference()
                .is_some_and(Self::is_any_compatible_non_array),
        }
    }

    /// type is just `anycompatiblerange`
    #[must_use]
    pub fn is_any_compatible_range(&self) -> bool {
        match self {
            Self::Concrete(c) => c.is_any_compatible_range(),
            Self::Dynamic(d) => d
                .type_definition()
                .type_reference()
                .is_some_and(Self::is_any_compatible_range),
        }
    }

    /// type is just `anycompatiblemultirange`
    #[must_use]
    pub fn is_any_compatible_multi_range(&self) -> bool {
        match self {
            Self::Concrete(c) => c.is_any_compatible_multi_range(),
            Self::Dynamic(d) => d
                .type_definition()
                .type_reference()
                .is_some_and(Self::is_any_compatible_multi_range),
        }
    }

    /// convert to range type
    #[must_use]
    pub fn into_range_type(&self) -> Self {
        match self {
            Self::Concrete(c) => c.get_range_type(),
            Self::Dynamic(_d) => todo!("range construction requires catalog semantics: #914"),
        }
    }

    /// convert to multi range type
    #[must_use]
    pub fn into_multi_range_type(&self) -> Self {
        match self {
            Self::Concrete(c) => c.get_multi_range_type(),
            Self::Dynamic(_d) => todo!("range construction requires catalog semantics: #914"),
        }
    }

    /// Substitute a simple-family placeholder, retaining owned result definitions.
    /// Fixed types are unchanged.
    pub fn apply_simple_infer_result(&mut self, infer_result_type: &Self) {
        if self.is_any_element() || self.is_any_non_array() {
            *self = infer_result_type.clone();
        } else if self.is_any_array() {
            *self = infer_result_type.array_type();
        } else if self.is_any_range() {
            *self = infer_result_type.into_range_type();
        } else if self.is_any_multi_range() {
            *self = infer_result_type.into_multi_range_type();
        }
    }

    /// Substitute a common-family placeholder, retaining owned result definitions.
    /// Fixed types are unchanged.
    pub fn apply_common_infer_result(&mut self, infer_result_type: &Self) {
        if self.is_any_compatible() || self.is_any_compatible_non_array() {
            *self = infer_result_type.clone();
        } else if self.is_any_compatible_array() {
            *self = infer_result_type.array_type();
        } else if self.is_any_compatible_range() {
            *self = infer_result_type.into_range_type();
        } else if self.is_any_compatible_multi_range() {
            *self = infer_result_type.into_multi_range_type();
        }
    }
}

#[cfg(test)]
mod test_structural_substitution {

    use crate::{
        sem::{PgBuiltInType, pg_type::type_ref::TypeReference},
        test_helpers::dynamic_types::record,
    };

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
}

#[cfg(test)]
mod test_owned_substitution {
    use crate::{
        names::FullName,
        sem::{PgBuiltInType, TypeDefinition, TypeReference},
        test_helpers::dynamic_types::record,
    };

    #[rstest::rstest]
    #[case::simple_element("anyelement", false, false)]
    #[case::simple_array("anyarray", false, true)]
    #[case::simple_nonarray("anynonarray", false, false)]
    #[case::common_element("anycompatible", true, false)]
    #[case::common_array("anycompatiblearray", true, true)]
    #[case::common_nonarray("anycompatiblenonarray", true, false)]
    fn owned_placeholders_preserve_structural_substitution(
        #[case] placeholder: &str,
        #[case] common: bool,
        #[case] array: bool,
    ) {
        let name = FullName::with_schema("pg_catalog", placeholder);
        let mut reference = TypeReference::dynamic(&TypeDefinition::builtin(&name, None));
        assert_eq!(reference.is_common_infer_arg(), common);
        assert_eq!(reference.is_simple_infer_arg(), !common);
        let row = record(&PgBuiltInType::int4());
        if common {
            reference.apply_common_infer_result(&row);
        } else {
            reference.apply_simple_infer_result(&row);
        }
        assert_eq!(reference, if array { row.array_type() } else { row });
    }
}

impl TypeReference {
    /// type is dynamic
    #[must_use]
    pub const fn is_dynamic(&self) -> bool {
        matches!(self, Self::Dynamic(_))
    }
}

impl std::fmt::Display for TypeReference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Concrete(c) => write!(f, "{c}"),
            Self::Dynamic(d) => d.type_definition().fmt(f),
        }
    }
}

#[cfg(test)]
mod dynamic_display_tests {
    use crate::{
        sem::{ColumnCollection, PgBuiltInType, TypeDefinition, TypeReference},
        test_helpers::dynamic_types::record,
    };

    #[rstest::rstest]
    #[case::row(false, "row(pg_catalog.int4)")]
    #[case::array(true, "row(pg_catalog.int4)[]")]
    fn structural_display_is_diagnostic_text(#[case] array: bool, #[case] expected: &str) {
        let row = record(&PgBuiltInType::int4());
        let reference = if array { row.array_type() } else { row };
        assert_eq!(reference.to_string(), expected);
        assert_eq!(record(&reference).to_string(), format!("row({expected})"));
        let empty = TypeDefinition::implicit_composite(&ColumnCollection::default());
        assert_eq!(TypeReference::dynamic(&empty).to_string(), "row()");
    }
}

impl std::str::FromStr for TypeReference {
    type Err = AnalysisError;

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
    use crate::{names::FullName, sem::pg_type::type_ref::TypeReference};

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

impl TypeReference {
    /// Resolve domain layers without erasing array element or source identity.
    /// Missing domain bases and cycles cannot establish coercion eligibility.
    pub(crate) fn domain_base_type(&self, context: &impl BaseParseContext) -> Option<Self> {
        let mut base = self.clone();
        let mut seen = std::collections::HashSet::new();
        loop {
            if base.is_dynamic() || base.is_array() == Some(true) {
                return Some(base);
            }
            if !seen.insert(base.clone()) {
                return None;
            }
            let Some(definition) = context.get_type_impl(base.full_name()?) else {
                // Ordinary names may be handled by cast lookup; a declared
                // domain, however, must reference an existing base definition.
                return if base == *self { Some(base) } else { None };
            };
            if !definition.is_domain() {
                return Some(base);
            }
            base = definition.domain_base()?.clone();
        }
    }
}

impl TypeReference {
    pub(crate) fn from_ast(ast: &TypeName) -> Result<Self, AnalysisError> {
        Ok(Self::concrete_type_ref(
            &FullName::from_ast_name(&ast.name)?,
            !ast.array_bounds.is_empty(),
        ))
    }
}

impl TypeReference {
    pub(crate) fn rule_shape_needs_modifiers(&self) -> bool {
        if self.is_dynamic() {
            return false;
        }
        let Some(name) = self.full_name() else {
            return false;
        };
        name.schema()
            .is_some_and(|schema| schema.as_str() == "pg_catalog")
            && matches!(
                name.name().as_str(),
                "varchar"
                    | "bpchar"
                    | "numeric"
                    | "time"
                    | "timetz"
                    | "timestamp"
                    | "timestamptz"
                    | "interval"
                    | "bit"
                    | "varbit"
            )
    }
}

impl TypeReference {
    /// Inconsistent type reference.
    #[track_caller]
    #[must_use]
    pub fn canonicalize_error(&self) -> AnalysisError {
        todo!()
    }
}
