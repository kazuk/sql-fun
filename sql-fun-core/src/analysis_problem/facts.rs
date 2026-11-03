//! Owned diagnostic facts, independent of the semantic AST and terminal layout.
use crate::analysis_problem::{OperatorName, TypeReference};
use std::fmt;

/// Known properties of an analyzed expression, without an internal expression dump.
#[derive(Debug, Clone, PartialEq, Eq, ::serde::Serialize, ::serde::Deserialize)]
#[cfg_attr(feature = "openrpc-schema", derive(::schemars::JsonSchema))]
pub struct SemScalarExpr {
    kind: String,
    identity: Option<String>,
    type_reference: Option<TypeReference>,
    not_null: Option<bool>,
}
impl SemScalarExpr {
    /// Retain the SQL construct and available type/nullability facts.
    #[must_use]
    pub fn new(kind: &str, type_reference: Option<&TypeReference>, not_null: Option<bool>) -> Self {
        Self {
            kind: kind.to_owned(),
            identity: None,
            type_reference: type_reference.cloned(),
            not_null,
        }
    }
    /// Retain a name or parameter position already known to semantic analysis.
    #[must_use]
    pub fn with_identity(mut self, identity: Option<&str>) -> Self {
        self.identity = identity.map(str::to_owned);
        self
    }
    /// Available SQL name or parameter position; not reconstructed SQL source.
    #[must_use]
    pub fn identity(&self) -> Option<&str> {
        self.identity.as_deref()
    }
    /// SQL construct description, rather than a Rust variant name.
    #[must_use]
    pub fn kind(&self) -> &str {
        &self.kind
    }
    /// Known type; absence does not establish SQL NULL or invalidity.
    #[must_use]
    pub const fn type_reference(&self) -> Option<&TypeReference> {
        self.type_reference.as_ref()
    }
    /// Known nullability, independently of the type.
    #[must_use]
    pub const fn not_null(&self) -> Option<bool> {
        self.not_null
    }
}
impl fmt::Display for SemScalarExpr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.kind)?;
        if let Some(identity) = &self.identity {
            write!(f, " {identity}")?;
        }
        match &self.type_reference {
            Some(ty) => write!(f, " of type {ty}"),
            None => f.write_str(" of undetermined type"),
        }
    }
}
#[cfg(test)]
mod expression_facts_tests {
    use crate::analysis_problem::facts::{OptionalExpression, SemScalarExpr, TypeReference};

    #[rstest::rstest]
    #[case::known(Some("pg_catalog.int4"), Some(true), "literal of type pg_catalog.int4")]
    #[case::unknown(None, None, "literal of undetermined type")]
    fn preserves_available_facts(
        #[case] ty: Option<&str>,
        #[case] not_null: Option<bool>,
        #[case] message: &str,
    ) {
        let value = SemScalarExpr::new("literal", ty.map(TypeReference::from).as_ref(), not_null);
        let serialized = ::serde_json::to_value(&value).unwrap();
        assert_eq!(serialized["type_reference"], ::serde_json::json!(ty));
        assert_eq!(serialized["not_null"], ::serde_json::json!(not_null));
        let restored: SemScalarExpr = ::serde_json::from_value(serialized).unwrap();
        assert_eq!(restored, value);
        assert_eq!(restored.to_string(), message);
        assert!(
            OptionalExpression::new(Some(&restored))
                .expression()
                .is_some()
        );
        assert!(OptionalExpression::new(None).expression().is_none());
    }
}

impl From<&Self> for SemScalarExpr {
    fn from(value: &Self) -> Self {
        value.clone()
    }
}

/// An operand's presence and known facts. An absent operand is not an unknown type.
#[derive(Debug, Clone, PartialEq, Eq, ::serde::Serialize, ::serde::Deserialize)]
#[cfg_attr(feature = "openrpc-schema", derive(::schemars::JsonSchema))]
pub struct OptionalExpression(Option<SemScalarExpr>);
impl OptionalExpression {
    /// Retain operand presence without a placeholder SQL expression.
    #[must_use]
    pub fn new(value: Option<&SemScalarExpr>) -> Self {
        Self(value.cloned())
    }
    /// Present operand, including its independently optional type.
    #[must_use]
    pub const fn expression(&self) -> Option<&SemScalarExpr> {
        self.0.as_ref()
    }
}

/// Ordered expression facts in an expression requiring a common type.
#[derive(Debug, Clone, PartialEq, Eq, ::serde::Serialize, ::serde::Deserialize)]
#[cfg_attr(feature = "openrpc-schema", derive(::schemars::JsonSchema))]
pub struct ExpressionList(Vec<SemScalarExpr>);
impl ExpressionList {
    /// Retain the operands in SQL order.
    #[must_use]
    pub fn new(values: &[SemScalarExpr]) -> Self {
        Self(values.to_vec())
    }
    /// Ordered operands.
    #[must_use]
    pub fn expressions(&self) -> &[SemScalarExpr] {
        &self.0
    }
}

/// Ordered known types of incompatible CASE results.
#[derive(Debug, Clone, PartialEq, Eq, ::serde::Serialize, ::serde::Deserialize)]
#[cfg_attr(feature = "openrpc-schema", derive(::schemars::JsonSchema))]
pub struct TypeList(Vec<TypeReference>);
impl TypeList {
    /// Retain the known types in analysis order.
    #[must_use]
    pub fn new(values: &[TypeReference]) -> Self {
        Self(values.to_vec())
    }
    /// Ordered known types.
    #[must_use]
    pub fn types(&self) -> &[TypeReference] {
        &self.0
    }
}

/// Ordered supplied argument types, retaining unknown arguments explicitly.
#[derive(Debug, Clone, PartialEq, Eq, ::serde::Serialize, ::serde::Deserialize)]
#[cfg_attr(feature = "openrpc-schema", derive(::schemars::JsonSchema))]
pub struct ArgumentBindingCollection(Vec<Option<TypeReference>>);
impl ArgumentBindingCollection {
    /// Retain argument order and unresolved types.
    #[must_use]
    pub fn new(values: &[Option<TypeReference>]) -> Self {
        Self(values.to_vec())
    }
    /// Supplied argument types; a missing entry type is not SQL NULL.
    #[must_use]
    pub fn types(&self) -> &[Option<TypeReference>] {
        &self.0
    }
}
impl fmt::Display for ArgumentBindingCollection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("(")?;
        for (index, ty) in self.0.iter().enumerate() {
            if index > 0 {
                f.write_str(", ")?;
            }
            match ty {
                Some(ty) => write!(f, "{ty}")?,
                None => f.write_str("undetermined type")?,
            }
        }
        f.write_str(")")
    }
}

/// A declared operator candidate signature, with its existing strictness fact.
#[derive(Debug, Clone, PartialEq, Eq, ::serde::Serialize, ::serde::Deserialize)]
#[cfg_attr(feature = "openrpc-schema", derive(::schemars::JsonSchema))]
pub struct OperatorSignature {
    operands: Vec<TypeReference>,
    result: TypeReference,
    strict: bool,
}
impl OperatorSignature {
    /// Retain a declaration without selecting a candidate.
    #[must_use]
    pub fn new(operands: &[TypeReference], result: &TypeReference, strict: bool) -> Self {
        Self {
            operands: operands.to_vec(),
            result: result.clone(),
            strict,
        }
    }
    /// Declared operand types in call order.
    #[must_use]
    pub fn operands(&self) -> &[TypeReference] {
        &self.operands
    }
    /// Declared result type.
    #[must_use]
    pub const fn result(&self) -> &TypeReference {
        &self.result
    }
    /// Whether the declaration is strict.
    #[must_use]
    pub const fn strict(&self) -> bool {
        self.strict
    }
}

/// The actual operator name and its declared overloads.
#[derive(Debug, Clone, PartialEq, Eq, ::serde::Serialize, ::serde::Deserialize)]
#[cfg_attr(feature = "openrpc-schema", derive(::schemars::JsonSchema))]
pub struct OperatorDefinition {
    name: OperatorName,
    signatures: Vec<OperatorSignature>,
    left_operand: bool,
    right_operand: bool,
}
impl OperatorDefinition {
    /// Preserve the producer's operator token and catalog candidates.
    #[must_use]
    pub fn new(
        name: impl Into<OperatorName>,
        signatures: &[OperatorSignature],
        left_operand: bool,
        right_operand: bool,
    ) -> Self {
        Self {
            name: name.into(),
            signatures: signatures.to_vec(),
            left_operand,
            right_operand,
        }
    }
    /// Whether this declaration takes an operand before the operator.
    #[must_use]
    pub const fn has_left_operand(&self) -> bool {
        self.left_operand
    }
    /// Whether this declaration takes an operand after the operator.
    #[must_use]
    pub const fn has_right_operand(&self) -> bool {
        self.right_operand
    }
    /// Operator selected explicitly by the SQL expression.
    #[must_use]
    pub const fn name(&self) -> &OperatorName {
        &self.name
    }
    /// Declared candidates, without implying a successful binding.
    #[must_use]
    pub fn signatures(&self) -> &[OperatorSignature] {
        &self.signatures
    }
}
impl fmt::Display for OperatorDefinition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.name.fmt(f)
    }
}

impl fmt::Display for TypeList {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, ty) in self.0.iter().enumerate() {
            if index > 0 {
                f.write_str(", ")?;
            }
            write!(f, "{ty}")?;
        }
        Ok(())
    }
}
impl fmt::Display for ExpressionList {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, expression) in self.0.iter().enumerate() {
            if index > 0 {
                f.write_str(", ")?;
            }
            match expression.type_reference() {
                Some(ty) => write!(f, "{ty}")?,
                None => f.write_str("undetermined type")?,
            }
        }
        Ok(())
    }
}
