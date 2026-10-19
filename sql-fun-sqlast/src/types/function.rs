//! Complete function signatures and pure operations on correlated overloads.

use crate::sem::{ColumnName, FullName};
use crate::types::{
    Candidates, ImplicitCompositeType, Member, Members, TypeDefinition, TypeReference,
};
use std::any::Any as StdAny;

/// Declared parameter mode, independent of a catalog or call binder.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct ParameterMode(u8);

impl ParameterMode {
    /// Input parameter.
    pub const fn input() -> Self {
        Self(0)
    }
    /// Output parameter.
    pub const fn output() -> Self {
        Self(1)
    }
    /// Input and output parameter.
    pub const fn in_out() -> Self {
        Self(2)
    }
    /// Variadic array input.
    pub const fn variadic() -> Self {
        Self(3)
    }
    /// RETURNS TABLE column.
    pub const fn table() -> Self {
        Self(4)
    }
    /// Whether this parameter accepts an input.
    pub const fn accepts_input(self) -> bool {
        matches!(self.0, 0 | 2 | 3)
    }
    /// Whether this parameter produces an output.
    pub const fn produces_output(self) -> bool {
        matches!(self.0, 1 | 2 | 4)
    }
}

/// An owned, unevaluated expression snapshot with an explicit representation.
///
/// SQL expression lists retain the entire list and an offset, so commas inside
/// expressions are never split heuristically. Structured snapshots retain every
/// serialized component. Equality is representation equality, not SQL equivalence.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DefaultExpression {
    representation: String,
    expression: serde_json::Value,
    index: usize,
}

impl DefaultExpression {
    /// Retain a structured expression in the producer's explicitly named format.
    pub fn snapshot(format: &str, expression: &serde_json::Value) -> Self {
        Self {
            representation: format.to_owned(),
            expression: expression.clone(),
            index: 0,
        }
    }
    /// Retain one expression's position in a complete SQL default-expression list.
    pub fn sql_list(sql: &str, index: usize) -> Self {
        Self {
            representation: "postgresql-sql-list".to_owned(),
            expression: serde_json::Value::String(sql.to_owned()),
            index,
        }
    }
    /// Representation identifier; no parsing or evaluation is implied.
    pub fn representation(&self) -> &str {
        &self.representation
    }
    /// Complete owned expression/list representation.
    pub const fn expression(&self) -> &serde_json::Value {
        &self.expression
    }
    /// Zero-based expression position within the retained list.
    pub const fn index(&self) -> usize {
        self.index
    }
}

/// A loaded parameter, retaining its declared name, mode, type and default.
#[derive(Debug, Clone, Eq)]
pub struct FunctionParameter {
    name: Option<ColumnName>,
    mode: ParameterMode,
    type_: TypeReference,
    default: Option<DefaultExpression>,
}

impl PartialEq for FunctionParameter {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.mode == other.mode
            && self.default == other.default
            && self.type_.same_type(other.type_.as_ref())
    }
}

impl FunctionParameter {
    /// Own the supplied loaded components; logical names are already SQL-normalized.
    pub fn new(
        name: &Option<ColumnName>,
        mode: ParameterMode,
        type_: &dyn TypeDefinition,
        default: &Option<DefaultExpression>,
    ) -> Self {
        Self {
            name: name.clone(),
            mode,
            type_: type_.clone_box(),
            default: default.clone(),
        }
    }
    /// Declared logical name, including exact case for quoted identifiers.
    pub const fn name(&self) -> Option<&ColumnName> {
        self.name.as_ref()
    }
    /// Declared parameter mode.
    pub const fn mode(&self) -> ParameterMode {
        self.mode
    }
    /// Complete loaded parameter type.
    pub fn parameter_type(&self) -> &dyn TypeDefinition {
        self.type_.as_ref()
    }
    /// Retained default expression, if declared.
    pub const fn default_expression(&self) -> Option<&DefaultExpression> {
        self.default.as_ref()
    }
}

/// The declared result, including its cardinality and procedure distinction.
///
/// OUT/INOUT/TABLE members remain in the signature's ordered parameters. An open
/// `record` result is a declared pseudo-type, not an invented empty composite.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct FunctionResult {
    type_: Option<TypeReference>,
    declared_type: Option<TypeReference>,
    returns_set: bool,
    procedure: bool,
}

impl FunctionResult {
    /// A function returning one value of the declared type (including `void`).
    pub fn scalar(type_: &dyn TypeDefinition) -> Self {
        Self {
            type_: Some(type_.clone_box()),
            declared_type: Some(type_.clone_box()),
            returns_set: false,
            procedure: false,
        }
    }
    /// A function returning a set of the declared type.
    pub fn set(type_: &dyn TypeDefinition) -> Self {
        Self {
            type_: Some(type_.clone_box()),
            declared_type: Some(type_.clone_box()),
            returns_set: true,
            procedure: false,
        }
    }
    /// A procedure with no declared function result; output parameters remain available.
    pub const fn procedure() -> Self {
        Self {
            type_: None,
            declared_type: None,
            returns_set: false,
            procedure: true,
        }
    }
    /// Compose a known record result from its ordered output parameters.
    /// Empty outputs retain the open declared record rather than an invented empty row.
    pub fn record(
        declaration: &dyn TypeDefinition,
        parameters: &[FunctionParameter],
        returns_set: bool,
    ) -> Self {
        let mut members = Vec::new();
        for parameter in parameters {
            if parameter.mode.produces_output() {
                members.push(Member::new(&parameter.name, parameter.type_.as_ref(), None));
            }
        }
        let value = if members.is_empty() {
            declaration.clone_box()
        } else {
            Box::new(ImplicitCompositeType::new(&Members::new(&members))) as TypeReference
        };
        Self {
            type_: Some(value),
            declared_type: Some(declaration.clone_box()),
            returns_set,
            procedure: false,
        }
    }
    /// Original declared type, before output-record composition.
    pub fn declared_type(&self) -> Option<&dyn TypeDefinition> {
        self.declared_type.as_deref()
    }
    /// Complete projected result type, absent only for procedures.
    pub fn result_type(&self) -> Option<&dyn TypeDefinition> {
        self.type_.as_deref()
    }
    /// Whether the function returns a set.
    pub const fn returns_set(&self) -> bool {
        self.returns_set
    }
    /// Whether this signature declares a procedure.
    pub const fn is_procedure(&self) -> bool {
        self.procedure
    }
}

/// A declared pseudo-type, distinct from a nominal base or an inference constraint.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PseudoType(FullName);

impl PseudoType {
    /// Preserve a known pseudo-type's qualified identity without asserting a base category.
    pub fn new(name: &FullName) -> Self {
        Self(name.clone())
    }
    /// Qualified declaration identity.
    pub const fn name(&self) -> &FullName {
        &self.0
    }
}

impl TypeDefinition for PseudoType {
    fn as_any(&self) -> &dyn StdAny {
        self
    }
    fn clone_box(&self) -> TypeReference {
        Box::new(self.clone())
    }
    fn same_type(&self, other: &dyn TypeDefinition) -> bool {
        other.as_any().downcast_ref::<Self>() == Some(self)
    }
}

/// An owned, complete signature. Identity includes its name and every component.
///
/// Parameter order is significant. Clone preserves polymorphic binding identity;
/// independently constructed scopes are distinct, without alpha-normalization.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct FunctionSignature {
    name: FullName,
    parameters: Vec<FunctionParameter>,
    result: FunctionResult,
    strict: bool,
    returns_not_null: bool,
}

impl FunctionSignature {
    /// Own complete components. This does not bind arguments or validate catalog membership.
    pub fn new(
        name: &FullName,
        parameters: &[FunctionParameter],
        result: &FunctionResult,
        strict: bool,
        returns_not_null: bool,
    ) -> Self {
        Self {
            name: name.clone(),
            parameters: parameters.to_vec(),
            result: result.clone(),
            strict,
            returns_not_null,
        }
    }
    /// Qualified function identity.
    pub const fn name(&self) -> &FullName {
        &self.name
    }
    /// All parameters in declaration order, including output-only parameters.
    pub fn parameters(&self) -> &[FunctionParameter] {
        &self.parameters
    }
    /// Complete declared result.
    pub const fn result(&self) -> &FunctionResult {
        &self.result
    }
    /// Whether the declaration is strict.
    pub const fn is_strict(&self) -> bool {
        self.strict
    }
    /// Retained return nullability property.
    pub const fn returns_not_null(&self) -> bool {
        self.returns_not_null
    }
    /// Find a callable input by zero-based input position, excluding OUT and TABLE.
    pub fn argument(&self, index: usize) -> Option<&FunctionParameter> {
        self.parameters
            .iter()
            .filter(|p| p.mode.accepts_input())
            .nth(index)
    }
    /// Find a callable input by its exact logical name.
    ///
    /// Duplicate callable names are an explicit error rather than an arbitrary match.
    pub fn argument_by_name(
        &self,
        name: &ColumnName,
    ) -> Result<Option<&FunctionParameter>, FunctionReadError> {
        let mut found = None;
        for p in &self.parameters {
            if p.mode.accepts_input() && p.name.as_ref() == Some(name) {
                if found.is_some() {
                    return Err(FunctionReadError::ambiguous_argument(name));
                }
                found = Some(p);
            }
        }
        Ok(found)
    }
}

impl TypeDefinition for FunctionSignature {
    fn contains_polymorphic(&self) -> bool {
        self.parameters
            .iter()
            .any(|p| p.type_.contains_polymorphic())
            || self
                .result
                .type_
                .as_ref()
                .is_some_and(|r| r.contains_polymorphic())
            || self
                .result
                .declared_type
                .as_ref()
                .is_some_and(|r| r.contains_polymorphic())
    }
    fn as_any(&self) -> &dyn StdAny {
        self
    }
    fn clone_box(&self) -> TypeReference {
        Box::new(self.clone())
    }
    fn same_type(&self, other: &dyn TypeDefinition) -> bool {
        other.as_any().downcast_ref::<Self>() == Some(self)
    }
}

/// An intrinsic read failed because a declaration has ambiguous callable names.
#[derive(Debug, Clone, Eq, PartialEq, ::thiserror::Error)]
#[error("ambiguous callable parameter: {0}")]
pub struct FunctionReadError(ColumnName);

impl FunctionReadError {
    /// Report a duplicate callable name without choosing one occurrence.
    pub fn ambiguous_argument(name: &ColumnName) -> Self {
        Self(name.clone())
    }
}

/// Correlated whole-signature candidates, retaining original order and duplicates.
///
/// Identity follows `Candidates`: set equality ignores order and duplicates.
/// Empty collections remain usable for subsequent projection/filtering. Every
/// leaf operation uses only same-type membership; none consults a context, expands
/// casts, binds polymorphic values, ranks overloads, or reconstructs signatures.
///
/// ```
/// use ::sql_fun_sqlast::sem::{ColumnName, FullName};
/// use ::sql_fun_sqlast::types::{
///     FunctionOverloads, FunctionParameter, FunctionResult, FunctionSignature,
///     ParameterMode, base::BaseType,
/// };
/// let integer = BaseType::new(&FullName::with_schema("pg_catalog", "int4"));
/// let parameter = FunctionParameter::new(
///     &Some(ColumnName::new("value")), ParameterMode::input(), &integer, &None,
/// );
/// let signature = FunctionSignature::new(
///     &FullName::with_schema("public", "identity"), &[parameter],
///     &FunctionResult::scalar(&integer), false, false,
/// );
/// let overloads = FunctionOverloads::new(&[signature]);
/// let remaining = overloads.filter_argument_by_name(&ColumnName::new("value"), &integer)?;
/// assert_eq!(remaining.signatures().len(), 1);
/// assert_eq!(remaining.filter_result(&integer), remaining);
/// # Ok::<(), sql_fun_sqlast::types::FunctionReadError>(())
/// ```
#[derive(Debug, Clone, Default)]
pub struct FunctionOverloads(Vec<FunctionSignature>);

impl FunctionOverloads {
    /// Clone complete signatures, preserving their scopes and original ordering.
    pub fn new(signatures: &[FunctionSignature]) -> Self {
        Self(signatures.to_vec())
    }
    /// Whole signatures in provider order, including duplicate snapshots.
    pub fn signatures(&self) -> &[FunctionSignature] {
        &self.0
    }
    /// Whether no signatures remain.
    pub const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    /// Convert the whole signatures to `Candidates`, or `Never` when empty.
    pub fn candidates(&self) -> TypeReference {
        let signatures: Vec<TypeReference> = self.0.iter().map(TypeDefinition::clone_box).collect();
        Candidates::from_candidates(&signatures)
    }
    /// Project a callable input position; missing positions contribute no type.
    pub fn argument_candidates(&self, index: usize) -> TypeReference {
        let values: Vec<TypeReference> = self
            .0
            .iter()
            .filter_map(|s| s.argument(index).map(|p| p.type_.clone()))
            .collect();
        Candidates::from_candidates(&values)
    }
    /// Project complete result values; procedures contribute no type.
    pub fn result_candidates(&self) -> TypeReference {
        let values: Vec<TypeReference> = self
            .0
            .iter()
            .filter_map(|s| s.result.type_.clone())
            .collect();
        Candidates::from_candidates(&values)
    }
    /// Project a named input separately in each signature; absent names contribute nothing.
    pub fn argument_candidates_by_name(
        &self,
        name: &ColumnName,
    ) -> Result<TypeReference, FunctionReadError> {
        let mut values = Vec::new();
        for s in &self.0 {
            if let Some(p) = s.argument_by_name(name)? {
                values.push(p.type_.clone());
            }
        }
        Ok(Candidates::from_candidates(&values))
    }
    /// Keep signatures whose selected input belongs to the supplied set.
    /// `Candidates` supplies its members; `Never` is empty; any other value is a singleton.
    pub fn filter_argument(&self, index: usize, candidates: &dyn TypeDefinition) -> Self {
        Self(
            self.0
                .iter()
                .filter(|s| {
                    s.argument(index)
                        .is_some_and(|p| Self::member(candidates, p.type_.as_ref()))
                })
                .cloned()
                .collect(),
        )
    }
    /// Keep signatures whose projected result belongs to the supplied set.
    pub fn filter_result(&self, candidates: &dyn TypeDefinition) -> Self {
        Self(
            self.0
                .iter()
                .filter(|s| {
                    s.result
                        .type_
                        .as_ref()
                        .is_some_and(|r| Self::member(candidates, r.as_ref()))
                })
                .cloned()
                .collect(),
        )
    }
    /// Keep signatures with a matching named input; absent names do not survive.
    pub fn filter_argument_by_name(
        &self,
        name: &ColumnName,
        candidates: &dyn TypeDefinition,
    ) -> Result<Self, FunctionReadError> {
        let mut signatures = Vec::new();
        for s in &self.0 {
            if s.argument_by_name(name)?
                .is_some_and(|p| Self::member(candidates, p.type_.as_ref()))
            {
                signatures.push(s.clone());
            }
        }
        Ok(Self(signatures))
    }
    fn member(set: &dyn TypeDefinition, value: &dyn TypeDefinition) -> bool {
        if let Some(set) = set.as_any().downcast_ref::<Candidates>() {
            set.contains(value)
        } else if set.as_any().is::<crate::types::Never>() {
            false
        } else {
            set.same_type(value)
        }
    }
}

impl PartialEq for FunctionOverloads {
    fn eq(&self, other: &Self) -> bool {
        self.candidates().same_type(other.candidates().as_ref())
    }
}
impl Eq for FunctionOverloads {}

#[cfg(test)]
mod tests;
