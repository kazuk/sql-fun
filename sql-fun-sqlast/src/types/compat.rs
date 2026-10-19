//! Explicit legacy interoperability. No existing semantic consumer opts in implicitly.
//!
//! Structural conversion accepts already loaded definitions. Named component
//! references and relation members require [`load_definition`] or [`load_reference`]
//! with the caller's `ParseContext`; a name alone cannot establish a base category.
//! Unsupported range/polymorphic/pseudo-type loading and incomplete or recursive
//! catalogs produce typed errors rather than fabricated terminal types.

mod cast;
mod context;
mod loaded_reference;

pub use self::cast::{
    load_assignment_cast, load_assignment_sources, load_cast_definition, load_cast_sources,
    load_explicit_cast, load_implicit_cast, load_implicit_castable,
};

use crate::sem::{
    ColumnCollection, ColumnDefinition, CreateCompositeType, CreateDomain, CreateEnum, FullName,
    ParseContext, SchemaFileContext, TableName, TypeDefinition as LegacyDefinition,
    TypeReference as LegacyReference,
};
use crate::types::{
    CompositeType, DomainType, EnumType, ImplicitCompositeType, Member, Members, TableRowType,
    TypeReference, array::ArrayType, base::BaseType,
};

/// Conversion cannot preserve the requested loaded type's components.
#[derive(Debug, Clone, Eq, PartialEq, ::thiserror::Error)]
pub enum ConversionError {
    /// A component needs an explicitly supplied catalog.
    #[error("type requires a ParseContext: {0}")]
    CatalogRequired(FullName),
    /// No definition exists for the selected type name.
    #[error("type not found: {0}")]
    TypeNotFound(FullName),
    /// The selected relation has no available table definition.
    #[error("relation not found: {0}")]
    RelationNotFound(FullName),
    /// The legacy domain has no underlying type name.
    #[error("domain has no underlying type: {0}")]
    MissingDomainBase(FullName),
    /// A field has no known type; the index is zero based.
    #[error("member at index {0} has no type")]
    MissingMemberType(usize),
    /// A named category cannot be loaded completely from the legacy data.
    #[error("complete category components unavailable for named type: {0}")]
    UnsupportedNamedType(FullName),
    /// A loaded category has no faithful legacy catalog reference.
    #[error("loaded type has no faithful legacy representation: {0}")]
    UnsupportedLoadedType(&'static str),
    /// Loaded identity components differ from the selected catalog snapshot.
    #[error("loaded type components do not match the selected catalog")]
    LoadedTypeMismatch,
    /// A declaration's own identity is dynamic rather than a nominal reference.
    #[error("declaration identity requires a concrete named reference")]
    UnsupportedIdentityReference,
    /// A component refers back to a type currently being loaded.
    #[error("recursive type cannot be represented as a finite owned value: {0}")]
    RecursiveType(FullName),
    /// A deeply nested value exceeds the bounded conversion stack.
    #[error("type nesting exceeds the conversion depth limit")]
    DepthLimit,
}

impl ConversionError {
    /// Report that an explicit catalog is needed.
    pub fn catalog_required(name: &FullName) -> Self {
        Self::CatalogRequired(name.clone())
    }
    /// Report an unavailable type.
    pub fn type_not_found(name: &FullName) -> Self {
        Self::TypeNotFound(name.clone())
    }
    /// Report an unavailable relation.
    pub fn relation_not_found(name: &FullName) -> Self {
        Self::RelationNotFound(name.clone())
    }
    /// Report an incomplete domain declaration.
    pub fn missing_domain_base(name: &FullName) -> Self {
        Self::MissingDomainBase(name.clone())
    }
    /// Report an incomplete member.
    pub const fn missing_member_type(index: usize) -> Self {
        Self::MissingMemberType(index)
    }
    /// Report a category outside this seam's loading support.
    pub fn unsupported_named_type(name: &FullName) -> Self {
        Self::UnsupportedNamedType(name.clone())
    }
    /// Report a loaded shape that cannot be represented by the legacy model.
    pub const fn unsupported_loaded_type(reason: &'static str) -> Self {
        Self::UnsupportedLoadedType(reason)
    }
    /// Report a loaded snapshot that differs from the selected catalog.
    pub const fn loaded_type_mismatch() -> Self {
        Self::LoadedTypeMismatch
    }
    /// Report a declaration identity that cannot be interpreted as a name.
    pub const fn unsupported_identity_reference() -> Self {
        Self::UnsupportedIdentityReference
    }
    /// Report recursive loading.
    pub fn recursive_type(name: &FullName) -> Self {
        Self::RecursiveType(name.clone())
    }
    /// Report excessive structural nesting.
    pub const fn depth_limit() -> Self {
        Self::DepthLimit
    }
}

/// Structurally convert an already available definition without consulting storage.
///
/// Builtin terminal definitions establish their own category; enum labels and
/// dynamic structural components can also be converted without a catalog.
/// Named components, domains and relation rows may return `CatalogRequired`.
pub fn from_legacy_definition(value: &LegacyDefinition) -> Result<TypeReference, ConversionError> {
    Loader::<SchemaFileContext>::structural().definition(value)
}

/// Structurally convert a reference; concrete names require an explicit catalog.
pub fn from_legacy_reference(value: &LegacyReference) -> Result<TypeReference, ConversionError> {
    Loader::<SchemaFileContext>::structural().reference(value)
}

/// Load a definition obtained from the caller's selected `ParseContext`.
///
/// Named nested components and relation members are loaded through that same
/// context. Cycles and unsupported/incomplete definitions are rejected explicitly.
pub fn load_definition<C: ParseContext>(
    context: &C,
    value: &LegacyDefinition,
) -> Result<TypeReference, ConversionError> {
    Loader::with_context(context).definition(value)
}

/// Resolve and load a concrete or dynamic legacy reference through the selected context.
pub fn load_reference<C: ParseContext>(
    context: &C,
    value: &LegacyReference,
) -> Result<TypeReference, ConversionError> {
    Loader::with_context(context).reference(value)
}

struct Loader<'a, C> {
    context: Option<&'a C>,
    active: Vec<FullName>,
    depth: usize,
}

impl<'a, C: ParseContext> Loader<'a, C> {
    const fn structural() -> Self {
        Self {
            context: None,
            active: Vec::new(),
            depth: 0,
        }
    }

    const fn with_context(context: &'a C) -> Self {
        Self {
            context: Some(context),
            active: Vec::new(),
            depth: 0,
        }
    }

    fn definition(&mut self, value: &LegacyDefinition) -> Result<TypeReference, ConversionError> {
        // A catalog may retain an unqualified declaration under a resolved key.
        let declared_name = match value {
            LegacyDefinition::Enum(value) => Some(value.name()),
            LegacyDefinition::CompositeType(value) => match value.type_ref() {
                Some(LegacyReference::Concrete(reference)) => Some(reference.full_name()),
                _ => None,
            },
            LegacyDefinition::Domain(value) => match value.type_ref() {
                LegacyReference::Concrete(reference) => Some(reference.full_name()),
                LegacyReference::Dynamic(_) => None,
            },
            LegacyDefinition::Builtin(LegacyReference::Concrete(value)) => Some(value.full_name()),
            _ => None,
        };
        let resolved = match (self.context, declared_name) {
            (Some(context), Some(name)) => {
                let resolved = context.resolve_type_name(name);
                if resolved.is_none() && name.schema().is_none() {
                    return Err(ConversionError::type_not_found(name));
                }
                resolved
            }
            _ => None,
        };
        self.definition_named(value, resolved.as_ref())
    }

    fn definition_named(
        &mut self,
        value: &LegacyDefinition,
        resolved: Option<&FullName>,
    ) -> Result<TypeReference, ConversionError> {
        if self.depth >= 128 {
            return Err(ConversionError::depth_limit());
        }
        self.depth += 1;
        let result = match value {
            LegacyDefinition::Builtin(reference) => self.builtin(reference, resolved),
            LegacyDefinition::Enum(value) => Ok(Self::enumeration(value, resolved)),
            LegacyDefinition::CompositeType(value) => self.composite(value, resolved),
            LegacyDefinition::Domain(value) => self.domain(value, resolved),
            LegacyDefinition::TableRef(table, reference) => self.table(table, reference),
            LegacyDefinition::ImplicitComposite(columns) => {
                let members = self.columns(columns)?;
                Ok(Box::new(ImplicitCompositeType::new(&members)) as TypeReference)
            }
        };
        self.depth -= 1;
        result
    }

    fn reference(&mut self, value: &LegacyReference) -> Result<TypeReference, ConversionError> {
        match value {
            LegacyReference::Concrete(value) => {
                let loaded = self.named(value.full_name())?;
                Ok(Self::array_if(loaded, value.is_array()))
            }
            LegacyReference::Dynamic(value) => self.definition(value.type_definition()),
        }
    }

    fn named(&mut self, name: &FullName) -> Result<TypeReference, ConversionError> {
        let Some(context) = self.context else {
            return Err(ConversionError::catalog_required(name));
        };
        let resolved = context
            .resolve_type_name(name)
            .ok_or_else(|| ConversionError::type_not_found(name))?;
        if self.active.contains(&resolved) {
            return Err(ConversionError::recursive_type(&resolved));
        }
        let definition = context
            .get_type_impl(&resolved)
            .ok_or_else(|| ConversionError::type_not_found(&resolved))?;
        self.active.push(resolved.clone());
        let result = self.definition_named(definition, Some(&resolved));
        self.active.pop();
        result
    }

    fn builtin(
        &mut self,
        value: &LegacyReference,
        resolved: Option<&FullName>,
    ) -> Result<TypeReference, ConversionError> {
        match value {
            LegacyReference::Concrete(value) => {
                let name = resolved.unwrap_or_else(|| value.full_name());
                // Legacy builtin records contain names, not range or inference
                // components. Do not manufacture those components from spelling.
                let pg_catalog = name
                    .schema()
                    .is_some_and(|schema| schema.as_str() == "pg_catalog");
                let special = name.is_record()
                    || name.is_simple_infer_arg()
                    || name.is_common_infer_arg()
                    || (pg_catalog
                        && matches!(
                            name.local_name().as_str(),
                            "int4range"
                                | "int8range"
                                | "numrange"
                                | "tsrange"
                                | "tstzrange"
                                | "daterange"
                                | "int4multirange"
                                | "int8multirange"
                                | "nummultirange"
                                | "tsmultirange"
                                | "tstzmultirange"
                                | "datemultirange"
                                | "any"
                                | "unknown"
                                | "void"
                                | "trigger"
                                | "event_trigger"
                                | "internal"
                                | "cstring"
                                | "anyenum"
                                | "language_handler"
                                | "fdw_handler"
                                | "tsm_handler"
                                | "table_am_handler"
                                | "index_am_handler"
                                | "pg_ddl_command"
                                | "opaque"
                        ));
                if special {
                    return Err(ConversionError::unsupported_named_type(name));
                }
                let terminal: TypeReference = Box::new(BaseType::new(name));
                Ok(Self::array_if(terminal, value.is_array()))
            }
            LegacyReference::Dynamic(value) => self.definition(value.type_definition()),
        }
    }

    fn array_if(value: TypeReference, is_array: bool) -> TypeReference {
        if is_array {
            Box::new(ArrayType::new(value.as_ref()))
        } else {
            value
        }
    }

    fn enumeration(value: &CreateEnum, resolved: Option<&FullName>) -> TypeReference {
        let mut labels = Vec::new();
        for label in value.values() {
            labels.push(label.name().to_owned());
        }
        Box::new(EnumType::new(
            resolved.unwrap_or_else(|| value.name()),
            &labels,
        ))
    }

    fn domain(
        &mut self,
        value: &CreateDomain,
        resolved: Option<&FullName>,
    ) -> Result<TypeReference, ConversionError> {
        let LegacyReference::Concrete(reference) = value.type_ref() else {
            return Err(ConversionError::unsupported_identity_reference());
        };
        let name = resolved.unwrap_or_else(|| reference.full_name());
        let base = value
            .base_type()
            .ok_or_else(|| ConversionError::missing_domain_base(name))?;
        let loaded = self.named(base)?;
        let domain: TypeReference = Box::new(DomainType::new(name, loaded.as_ref()));
        Ok(Self::array_if(domain, reference.is_array()))
    }

    fn composite(
        &mut self,
        value: &CreateCompositeType,
        resolved: Option<&FullName>,
    ) -> Result<TypeReference, ConversionError> {
        let members = self.columns(&value.get_column_collection())?;
        let (declared_name, is_array) = match value.type_ref() {
            Some(LegacyReference::Concrete(reference)) => {
                (Some(reference.full_name()), reference.is_array())
            }
            Some(LegacyReference::Dynamic(_)) => {
                return Err(ConversionError::unsupported_identity_reference());
            }
            None => (None, false),
        };
        let name = if let Some(resolved) = resolved {
            Some(resolved)
        } else {
            declared_name
        };
        let composite: TypeReference = Box::new(CompositeType::new(&name.cloned(), &members));
        Ok(Self::array_if(composite, is_array))
    }

    fn columns(&mut self, columns: &ColumnCollection) -> Result<Members, ConversionError> {
        let mut members = Vec::new();
        while let Some(column) = columns.get_at(members.len()) {
            members.push(self.column(column, members.len())?);
        }
        Ok(Members::new(&members))
    }

    fn column(
        &mut self,
        value: &ColumnDefinition,
        index: usize,
    ) -> Result<Member, ConversionError> {
        let reference = value
            .get_type()
            .ok_or_else(|| ConversionError::missing_member_type(index))?;
        let loaded = self.reference(reference)?;
        Ok(Member::new(
            value.name(),
            loaded.as_ref(),
            value.is_not_null(),
        ))
    }

    fn table(
        &mut self,
        name: &TableName,
        reference: &LegacyReference,
    ) -> Result<TypeReference, ConversionError> {
        let Some(context) = self.context else {
            return Err(ConversionError::catalog_required(name.full_name()));
        };
        // Legacy table references carry the row's array qualifier separately.
        // A dynamic reference can hold another full definition; preserve that
        // payload only if it describes this same table row rather than discard it.
        let LegacyReference::Concrete(reference) = reference else {
            return Err(ConversionError::unsupported_named_type(name.full_name()));
        };
        let resolved = context
            .resolve_table_name(name)
            .ok_or_else(|| ConversionError::relation_not_found(name.full_name()))?;
        if reference.full_name() != name.full_name() {
            return Err(ConversionError::unsupported_named_type(
                reference.full_name(),
            ));
        }
        let table = context
            .get_table_impl(resolved.full_name())
            .ok_or_else(|| ConversionError::relation_not_found(resolved.full_name()))?;
        let mut members = Vec::new();
        while let Some(column) = table.get_column_at(members.len()) {
            members.push(self.column(column, members.len())?);
        }
        let row: TypeReference = Box::new(TableRowType::new(
            resolved.full_name(),
            &Members::new(&members),
        ));
        Ok(Self::array_if(row, reference.is_array()))
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod cast_tests;

#[cfg(test)]
mod context_tests;
