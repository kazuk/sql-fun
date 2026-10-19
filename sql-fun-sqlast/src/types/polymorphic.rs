//! Catalog-independent PostgreSQL signature variables and structural links.

use crate::sem::FullName;
use crate::types::{TypeDefinition, TypeReference};
use std::any::Any;
use std::sync::Arc;

/// The two independent PostgreSQL polymorphic binding policies.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum PolymorphicFamily {
    /// Exact matching within a call.
    Simple,
    /// Selection of a common type, without executing that selection here.
    Common,
}

impl PolymorphicFamily {
    /// Exact-type family.
    pub const fn simple() -> Self {
        Self::Simple
    }
    /// Common-type family.
    pub const fn common() -> Self {
        Self::Common
    }
    const fn index(self) -> usize {
        match self {
            Self::Simple => 0,
            Self::Common => 1,
        }
    }
}

/// A named polymorphic category, distinct from unconstrained `Any` and `unknown`.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum PolymorphicCategory {
    /// PostgreSQL `anyelement`.
    Element,
    /// PostgreSQL `anyarray`.
    Array,
    /// PostgreSQL `anynonarray`.
    Nonarray,
    /// PostgreSQL `anyenum`.
    Enum,
    /// PostgreSQL `anyrange`.
    Range,
    /// PostgreSQL `anymultirange`.
    Multirange,
    /// PostgreSQL `anycompatible`.
    Compatible,
    /// PostgreSQL `anycompatiblearray`.
    CompatibleArray,
    /// PostgreSQL `anycompatiblenonarray`.
    CompatibleNonarray,
    /// PostgreSQL `anycompatiblerange`.
    CompatibleRange,
    /// PostgreSQL `anycompatiblemultirange`.
    CompatibleMultirange,
}

impl PolymorphicCategory {
    /// PostgreSQL `anyelement`.
    pub const fn anyelement() -> Self {
        Self::Element
    }
    /// PostgreSQL `anyarray`.
    pub const fn anyarray() -> Self {
        Self::Array
    }
    /// PostgreSQL `anynonarray`.
    pub const fn anynonarray() -> Self {
        Self::Nonarray
    }
    /// PostgreSQL `anyenum`.
    pub const fn anyenum() -> Self {
        Self::Enum
    }
    /// PostgreSQL `anyrange`.
    pub const fn anyrange() -> Self {
        Self::Range
    }
    /// PostgreSQL `anymultirange`.
    pub const fn anymultirange() -> Self {
        Self::Multirange
    }
    /// PostgreSQL `anycompatible`.
    pub const fn anycompatible() -> Self {
        Self::Compatible
    }
    /// PostgreSQL `anycompatiblearray`.
    pub const fn anycompatiblearray() -> Self {
        Self::CompatibleArray
    }
    /// PostgreSQL `anycompatiblenonarray`.
    pub const fn anycompatiblenonarray() -> Self {
        Self::CompatibleNonarray
    }
    /// PostgreSQL `anycompatiblerange`.
    pub const fn anycompatiblerange() -> Self {
        Self::CompatibleRange
    }
    /// PostgreSQL `anycompatiblemultirange`.
    pub const fn anycompatiblemultirange() -> Self {
        Self::CompatibleMultirange
    }
    /// Interpret an exact logical PostgreSQL identifier, not raw SQL text.
    ///
    /// Callers must perform SQL identifier unquoting/case folding first. Unknown
    /// names (including `any`, `unknown`, and `anycompatibleenum`) return `None`.
    pub fn from_postgres_identifier(name: &str) -> Option<Self> {
        match name {
            "anyelement" => Some(Self::Element),
            "anyarray" => Some(Self::Array),
            "anynonarray" => Some(Self::Nonarray),
            "anyenum" => Some(Self::Enum),
            "anyrange" => Some(Self::Range),
            "anymultirange" => Some(Self::Multirange),
            "anycompatible" => Some(Self::Compatible),
            "anycompatiblearray" => Some(Self::CompatibleArray),
            "anycompatiblenonarray" => Some(Self::CompatibleNonarray),
            "anycompatiblerange" => Some(Self::CompatibleRange),
            "anycompatiblemultirange" => Some(Self::CompatibleMultirange),
            _ => None,
        }
    }
    /// Interpret only an explicitly resolved `pg_catalog` name, without lookup.
    pub fn from_postgres_name(name: &FullName) -> Option<Self> {
        if name.catalog().is_some() || !name.schema().is_some_and(|s| s.as_str() == "pg_catalog") {
            return None;
        }
        Self::from_postgres_identifier(name.local_name().as_str())
    }
    /// The exact PostgreSQL spelling.
    pub const fn spelling(self) -> &'static str {
        match self {
            Self::Element => "anyelement",
            Self::Array => "anyarray",
            Self::Nonarray => "anynonarray",
            Self::Enum => "anyenum",
            Self::Range => "anyrange",
            Self::Multirange => "anymultirange",
            Self::Compatible => "anycompatible",
            Self::CompatibleArray => "anycompatiblearray",
            Self::CompatibleNonarray => "anycompatiblenonarray",
            Self::CompatibleRange => "anycompatiblerange",
            Self::CompatibleMultirange => "anycompatiblemultirange",
        }
    }
    /// The independent binding policy of this category.
    pub const fn family(self) -> PolymorphicFamily {
        match self {
            Self::Element
            | Self::Array
            | Self::Nonarray
            | Self::Enum
            | Self::Range
            | Self::Multirange => PolymorphicFamily::simple(),
            _ => PolymorphicFamily::common(),
        }
    }
    const fn component(self) -> Component {
        match self {
            Self::Array | Self::CompatibleArray => Component::array(),
            Self::Range | Self::CompatibleRange => Component::range(),
            Self::Multirange | Self::CompatibleMultirange => Component::multirange(),
            _ => Component::element(),
        }
    }
    const fn nonarray(self) -> bool {
        matches!(self, Self::Nonarray | Self::CompatibleNonarray | Self::Enum)
    }
    const fn enumeration(self) -> bool {
        matches!(self, Self::Enum)
    }
    const fn range_like(self) -> bool {
        self.component().is_range_like()
    }
    const fn array(self) -> bool {
        self.component().is_array()
    }
}

/// The direction of a polymorphic occurrence in a signature.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum PolymorphicRole {
    /// An ordinary input argument.
    Input,
    /// A result, OUT parameter, or table output column.
    Output,
    /// An INOUT parameter.
    InOut,
    /// A variadic array input, related to repeated element inputs.
    Variadic,
}

impl PolymorphicRole {
    /// An input argument.
    pub const fn input() -> Self {
        Self::Input
    }
    /// A result or output argument.
    pub const fn output() -> Self {
        Self::Output
    }
    /// An input/output argument.
    pub const fn in_out() -> Self {
        Self::InOut
    }
    /// A variadic array input.
    pub const fn variadic() -> Self {
        Self::Variadic
    }
    const fn input_capable(self) -> bool {
        !matches!(self, Self::Output)
    }
    const fn output_capable(self) -> bool {
        matches!(self, Self::Output | Self::InOut)
    }
    /// Whether the declaration has variadic array-to-element relationships.
    /// This alone does not impose an unconditional `anynonarray` restriction.
    pub const fn is_variadic(self) -> bool {
        matches!(self, Self::Variadic)
    }
}

/// One category and role supplied to atomic signature construction.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct PolymorphicPosition {
    category: PolymorphicCategory,
    role: PolymorphicRole,
}

impl PolymorphicPosition {
    /// Declare one occurrence; no catalog or actual call type is required.
    pub const fn new(category: PolymorphicCategory, role: PolymorphicRole) -> Self {
        Self { category, role }
    }
    /// The occurrence category.
    pub const fn category(self) -> PolymorphicCategory {
        self.category
    }
    /// The occurrence role.
    pub const fn role(self) -> PolymorphicRole {
        self.role
    }
}

/// An invalid signature declaration, not an unresolved call binding.
#[derive(Debug, Clone, Eq, PartialEq, ::thiserror::Error)]
pub enum PolymorphicError {
    /// A result family has no input supplying that family's type.
    #[error("polymorphic output at position {0} has no input in its family")]
    MissingInput(usize),
    /// A range result cannot be reconstructed from only an element subtype.
    #[error("range or multirange output at position {0} requires a range or multirange input")]
    MissingRangeInput(usize),
    /// Variadic polymorphic parameters must use an array category.
    #[error("variadic position {0} requires anyarray or anycompatiblearray")]
    InvalidVariadic(usize),
}

impl PolymorphicError {
    /// Missing family input.
    pub const fn missing_input(position: usize) -> Self {
        Self::MissingInput(position)
    }
    /// Missing range identity input.
    pub const fn missing_range_input(position: usize) -> Self {
        Self::MissingRangeInput(position)
    }
    /// Invalid variadic category.
    pub const fn invalid_variadic(position: usize) -> Self {
        Self::InvalidVariadic(position)
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum Component {
    Element,
    Array,
    Range,
    Multirange,
}

impl Component {
    const fn element() -> Self {
        Self::Element
    }
    const fn array() -> Self {
        Self::Array
    }
    const fn range() -> Self {
        Self::Range
    }
    const fn multirange() -> Self {
        Self::Multirange
    }
    const fn is_range_like(self) -> bool {
        matches!(self, Self::Range | Self::Multirange)
    }
    const fn is_array(self) -> bool {
        matches!(self, Self::Array)
    }
}

#[derive(Debug, Default)]
struct FamilyRestrictions {
    nonarray: bool,
    enumeration: bool,
}

#[derive(Debug)]
struct Scope {
    restrictions: [FamilyRestrictions; 2],
}

/// A shared signature variable. Identity is scoped, not a SQL cast decision.
#[derive(Debug, Clone)]
pub struct PolymorphicVariable {
    scope: Arc<Scope>,
    family: PolymorphicFamily,
    component: Component,
}

impl PartialEq for PolymorphicVariable {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.scope, &other.scope)
            && self.family == other.family
            && self.component == other.component
    }
}
impl Eq for PolymorphicVariable {}

impl PolymorphicVariable {
    fn related(&self, component: Component) -> Self {
        Self {
            scope: Arc::clone(&self.scope),
            family: self.family,
            component,
        }
    }
    /// The independent binding policy.
    pub const fn family(&self) -> PolymorphicFamily {
        self.family
    }
    /// Shared element variable (or array element/range subtype/multirange subtype).
    pub fn element(&self) -> Self {
        self.related(Component::element())
    }
    /// Shared array variable, related through its element.
    pub fn array(&self) -> Self {
        self.related(Component::array())
    }
    /// Shared range identity, not derivable uniquely from the subtype.
    pub fn range(&self) -> Self {
        self.related(Component::range())
    }
    /// Shared multirange identity, related to the family's range variable.
    pub fn multirange(&self) -> Self {
        self.related(Component::multirange())
    }
    /// Whether the element variable must be non-array, including enum restrictions.
    pub fn requires_nonarray(&self) -> bool {
        self.scope.restrictions[self.family.index()].nonarray
    }
    /// Whether the element variable must be an enum.
    pub fn requires_enum(&self) -> bool {
        self.scope.restrictions[self.family.index()].enumeration
    }
}

/// An immutable occurrence of a named PostgreSQL polymorphic constraint.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PolymorphicType {
    category: PolymorphicCategory,
    variable: PolymorphicVariable,
}

impl PolymorphicType {
    /// The spelling category, retaining nonarray/enum distinctions.
    pub const fn category(&self) -> PolymorphicCategory {
        self.category
    }
    /// The shared variable and its structural relationships/restrictions.
    pub const fn variable(&self) -> &PolymorphicVariable {
        &self.variable
    }
}

impl TypeDefinition for PolymorphicType {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn clone_box(&self) -> TypeReference {
        Box::new(self.clone())
    }
    fn same_type(&self, other: &dyn TypeDefinition) -> bool {
        other.as_any().downcast_ref::<Self>() == Some(self)
    }
    fn contains_polymorphic(&self) -> bool {
        true
    }
}

/// A finalized signature-local collection of linked polymorphic occurrences.
///
/// Construction is atomic. Clones preserve variable identity; independently
/// constructed signatures have distinct binding identities, even when their
/// declarations are textually identical. No actual binding or coercion runs.
///
/// ```
/// use sql_fun_sqlast::types::{PolymorphicCategory as C, PolymorphicPosition as P,
///     PolymorphicRole as R, SignaturePolymorphism};
/// let signature = SignaturePolymorphism::new(&[
///     P::new(C::anyarray(), R::input()),
///     P::new(C::anyelement(), R::output()),
/// ])?;
/// assert_eq!(signature.types()[0].variable().element(),
///            *signature.types()[1].variable());
/// # Ok::<(), sql_fun_sqlast::types::PolymorphicError>(())
/// ```
#[derive(Debug, Clone)]
pub struct SignaturePolymorphism {
    positions: Vec<PolymorphicPosition>,
    types: Vec<PolymorphicType>,
}

impl SignaturePolymorphism {
    /// Finalize all polymorphic arguments and results together.
    ///
    /// Positions include polymorphic occurrences only, in the caller's order.
    /// Unresolved call types are permitted; malformed declarations are errors.
    pub fn new(positions: &[PolymorphicPosition]) -> Result<Self, PolymorphicError> {
        let mut signature = Self {
            positions: positions.to_vec(),
            types: Vec::new(),
        };
        signature.finalize()?;
        Ok(signature)
    }

    fn finalize(&mut self) -> Result<(), PolymorphicError> {
        let mut restrictions = [FamilyRestrictions::default(), FamilyRestrictions::default()];
        let mut inputs = [false; 2];
        let mut ranges = [false; 2];
        for (index, position) in self.positions.iter().enumerate() {
            let family = position.category.family().index();
            restrictions[family].nonarray |= position.category.nonarray();
            restrictions[family].enumeration |= position.category.enumeration();
            if position.role.is_variadic() && !position.category.array() {
                return Err(PolymorphicError::invalid_variadic(index));
            }
            if position.role.input_capable() {
                inputs[family] = true;
                ranges[family] |= position.category.range_like();
            }
        }
        for (index, position) in self.positions.iter().enumerate() {
            let family = position.category.family().index();
            if position.role.output_capable() {
                if !inputs[family] {
                    return Err(PolymorphicError::missing_input(index));
                }
                if position.category.range_like() && !ranges[family] {
                    return Err(PolymorphicError::missing_range_input(index));
                }
            }
        }
        let scope = Arc::new(Scope { restrictions });
        self.types = self
            .positions
            .iter()
            .map(|position| PolymorphicType {
                category: position.category,
                variable: PolymorphicVariable {
                    scope: Arc::clone(&scope),
                    family: position.category.family(),
                    component: position.category.component(),
                },
            })
            .collect();
        Ok(())
    }
    /// Linked occurrences in declaration order.
    pub fn types(&self) -> &[PolymorphicType] {
        &self.types
    }
    /// Retained input/output/variadic declarations in the same order.
    pub fn positions(&self) -> &[PolymorphicPosition] {
        &self.positions
    }
}

#[cfg(test)]
mod tests;
