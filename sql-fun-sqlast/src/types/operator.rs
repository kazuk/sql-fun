//! Whole operator signatures and correlated operand filtering.

use crate::sem::OperatorName;
use crate::types::{Candidates, Never, TypeDefinition, TypeReference};
use std::any::Any as StdAny;

/// Syntactic operand presence, independent of the operand types.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct OperatorForm(u8);

impl OperatorForm {
    /// Two operands: `left OP right`.
    pub const fn binary() -> Self {
        Self(3)
    }
    /// A left operand only: `value OP` (postfix).
    pub const fn left_unary() -> Self {
        Self(1)
    }
    /// A right operand only: `OP value` (prefix).
    pub const fn right_unary() -> Self {
        Self(2)
    }
    /// Whether the syntax supplies a left operand.
    pub const fn has_left(self) -> bool {
        self.0 & 1 != 0
    }
    /// Whether the syntax supplies a right operand.
    pub const fn has_right(self) -> bool {
        self.0 & 2 != 0
    }
}

impl std::fmt::Display for OperatorForm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if *self == Self::binary() {
            f.write_str("binary")
        } else if *self == Self::left_unary() {
            f.write_str("left-unary (postfix)")
        } else {
            f.write_str("right-unary (prefix)")
        }
    }
}

/// A loaded operator declaration, with complete correlated operand/result values.
///
/// Name identity is the provider's available logical catalog key, not a claim of
/// canonical schema resolution. Legacy catalogs may contain unqualified keys.
#[derive(Debug, Clone)]
pub struct OperatorSignature {
    name: OperatorName,
    form: OperatorForm,
    operands: Vec<TypeReference>,
    result: TypeReference,
    strict: bool,
}

impl OperatorSignature {
    /// Create a binary declaration with operands in SQL order.
    pub fn binary(
        name: &OperatorName,
        left: &dyn TypeDefinition,
        right: &dyn TypeDefinition,
        result: &dyn TypeDefinition,
        strict: bool,
    ) -> Self {
        Self::new(
            name,
            OperatorForm::binary(),
            &[left.clone_box(), right.clone_box()],
            result,
            strict,
        )
    }
    /// Create a postfix declaration (`value OP`).
    pub fn left_unary(
        name: &OperatorName,
        operand: &dyn TypeDefinition,
        result: &dyn TypeDefinition,
        strict: bool,
    ) -> Self {
        Self::new(
            name,
            OperatorForm::left_unary(),
            &[operand.clone_box()],
            result,
            strict,
        )
    }
    /// Create a prefix declaration (`OP value`).
    pub fn right_unary(
        name: &OperatorName,
        operand: &dyn TypeDefinition,
        result: &dyn TypeDefinition,
        strict: bool,
    ) -> Self {
        Self::new(
            name,
            OperatorForm::right_unary(),
            &[operand.clone_box()],
            result,
            strict,
        )
    }
    fn new(
        name: &OperatorName,
        form: OperatorForm,
        operands: &[TypeReference],
        result: &dyn TypeDefinition,
        strict: bool,
    ) -> Self {
        Self {
            name: name.clone(),
            form,
            operands: operands.to_vec(),
            result: result.clone_box(),
            strict,
        }
    }
    /// Available catalog name/key, preserving explicit qualification.
    pub const fn name(&self) -> &OperatorName {
        &self.name
    }
    /// Syntactic operand presence.
    pub const fn form(&self) -> OperatorForm {
        self.form
    }
    /// Operand in syntax order; a unary operand is always at index zero.
    pub fn operand(&self, index: usize) -> Option<&dyn TypeDefinition> {
        self.operands.get(index).map(AsRef::as_ref)
    }
    /// Left operand, absent for prefix syntax.
    pub fn left_operand(&self) -> Option<&dyn TypeDefinition> {
        if self.form.has_left() {
            self.operand(0)
        } else {
            None
        }
    }
    /// Right operand, absent for postfix syntax.
    pub fn right_operand(&self) -> Option<&dyn TypeDefinition> {
        if self.form.has_right() {
            self.operand(usize::from(self.form.has_left()))
        } else {
            None
        }
    }
    /// Complete declared result type.
    pub fn result(&self) -> &dyn TypeDefinition {
        self.result.as_ref()
    }
    /// Whether a NULL operand forces a NULL result.
    pub const fn is_strict(&self) -> bool {
        self.strict
    }
}

impl PartialEq for OperatorSignature {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.form == other.form
            && self.strict == other.strict
            && self.operands.len() == other.operands.len()
            && self
                .operands
                .iter()
                .zip(&other.operands)
                .all(|(a, b)| a.same_type(b.as_ref()))
            && self.result.same_type(other.result.as_ref())
    }
}
impl Eq for OperatorSignature {}

impl TypeDefinition for OperatorSignature {
    fn contains_polymorphic(&self) -> bool {
        self.operands.iter().any(|v| v.contains_polymorphic()) || self.result.contains_polymorphic()
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

#[cfg(test)]
mod signature_identity_tests {
    use super::OperatorSignature;
    use crate::sem::{FullName, OperatorName};
    use crate::types::{TypeDefinition, array::ArrayType, base::BaseType};

    #[rstest::rstest]
    #[case::name(0)]
    #[case::order(1)]
    #[case::result(2)]
    #[case::strictness(3)]
    #[case::form(4)]
    fn identity_includes_every_component(#[case] changed: usize) {
        let a = BaseType::new(&FullName::from_local_name("a"));
        let b = ArrayType::new(&a);
        let name = OperatorName::from_parts(Some("public"), "~");
        let original = OperatorSignature::binary(&name, &a, &b, &a, true);
        let other = match changed {
            0 => OperatorSignature::binary(&OperatorName::from_parts(None, "~"), &a, &b, &a, true),
            1 => OperatorSignature::binary(&name, &b, &a, &a, true),
            2 => OperatorSignature::binary(&name, &a, &b, &b, true),
            3 => OperatorSignature::binary(&name, &a, &b, &a, false),
            _ => OperatorSignature::right_unary(&name, &a, &a, true),
        };
        assert!(original.same_type(original.clone_box().as_ref()));
        assert!(!original.same_type(&other));
        assert!(!other.same_type(&original));
        assert!(!original.same_type(&a));
        assert!(!original.contains_polymorphic());
        assert!(
            !OperatorSignature::left_unary(&name, &a, &a, true)
                .same_type(&OperatorSignature::right_unary(&name, &a, &a, true))
        );
    }
}

/// Candidate types for each syntactically supplied operand, with an explicit form.
#[derive(Debug, Clone)]
pub struct OperatorCall {
    form: OperatorForm,
    operands: Vec<TypeReference>,
}

impl OperatorCall {
    /// Binary invocation; both supplied candidate values must match one signature.
    pub fn binary(left: &dyn TypeDefinition, right: &dyn TypeDefinition) -> Self {
        Self {
            form: OperatorForm::binary(),
            operands: vec![left.clone_box(), right.clone_box()],
        }
    }
    /// Postfix invocation (`value OP`).
    pub fn left_unary(operand: &dyn TypeDefinition) -> Self {
        Self {
            form: OperatorForm::left_unary(),
            operands: vec![operand.clone_box()],
        }
    }
    /// Prefix invocation (`OP value`).
    pub fn right_unary(operand: &dyn TypeDefinition) -> Self {
        Self {
            form: OperatorForm::right_unary(),
            operands: vec![operand.clone_box()],
        }
    }
    fn accepts(&self, signature: &OperatorSignature) -> bool {
        self.form == signature.form
            && self.operands.len() == signature.operands.len()
            && self
                .operands
                .iter()
                .zip(&signature.operands)
                .all(|(set, value)| OperatorOverloads::member(set.as_ref(), value.as_ref()))
    }
}

/// Complete operator candidates in provider order, including duplicate snapshots.
///
/// Identity uses candidate-set equality, ignoring order/duplicates. All filtering
/// retains whole signatures, without casts, binding, ranking or automatic choice.
///
/// ```
/// use sql_fun_sqlast::sem::{FullName, OperatorName};
/// use sql_fun_sqlast::types::{OperatorCall, OperatorOverloads, OperatorSignature, base::BaseType};
/// let int = BaseType::new(&FullName::with_schema("pg_catalog", "int4"));
/// let text = BaseType::new(&FullName::with_schema("pg_catalog", "text"));
/// let name = OperatorName::from_parts(None, "+");
/// let overloads = OperatorOverloads::new(&[
///     OperatorSignature::binary(&name, &int, &text, &int, true),
///     OperatorSignature::binary(&name, &text, &int, &text, false),
/// ]);
/// assert!(overloads.match_call(&OperatorCall::binary(&int, &int)).is_empty());
/// let matched = overloads.match_call(&OperatorCall::binary(&int, &text));
/// assert_eq!(matched.overloads().signatures().len(), 1);
/// let accepted_right = matched.operand_candidates(1);
/// let results = matched.result_candidates();
/// assert!(!matched.filter_result(&int).is_empty());
/// ```
#[derive(Debug, Clone, Default)]
pub struct OperatorOverloads(Vec<OperatorSignature>);

impl OperatorOverloads {
    /// Clone whole signatures, preserving their scopes and provider order.
    pub fn new(signatures: &[OperatorSignature]) -> Self {
        Self(signatures.to_vec())
    }
    /// Whole original signatures, including duplicate snapshots.
    pub fn signatures(&self) -> &[OperatorSignature] {
        &self.0
    }
    /// Whether no signatures remain.
    pub const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    /// Whole-signature candidates, or `Never` for an empty collection.
    pub fn candidates(&self) -> TypeReference {
        let values: Vec<_> = self.0.iter().map(TypeDefinition::clone_box).collect();
        Candidates::from_candidates(&values)
    }
    /// Types at a syntax-order operand index; missing positions contribute nothing.
    pub fn operand_candidates(&self, index: usize) -> TypeReference {
        let values: Vec<_> = self
            .0
            .iter()
            .filter_map(|s| s.operand(index).map(TypeDefinition::clone_box))
            .collect();
        Candidates::from_candidates(&values)
    }
    /// Result types of whole surviving signatures.
    pub fn result_candidates(&self) -> TypeReference {
        let values: Vec<_> = self.0.iter().map(|s| s.result().clone_box()).collect();
        Candidates::from_candidates(&values)
    }
    /// Retain every signature whose supplied operand directly belongs to candidates.
    pub fn filter_operand(&self, index: usize, candidates: &dyn TypeDefinition) -> Self {
        Self(
            self.0
                .iter()
                .filter(|s| {
                    s.operand(index)
                        .is_some_and(|v| Self::member(candidates, v))
                })
                .cloned()
                .collect(),
        )
    }
    /// Retain every signature whose result directly belongs to candidates.
    pub fn filter_result(&self, candidates: &dyn TypeDefinition) -> Self {
        Self(
            self.0
                .iter()
                .filter(|s| Self::member(candidates, s.result()))
                .cloned()
                .collect(),
        )
    }
    /// Match the explicit form and all operands together; never combine projections.
    /// A `Never` input has no candidates; it does not mean an absent operand.
    pub fn match_call(&self, call: &OperatorCall) -> OperatorCallMatches {
        OperatorCallMatches(Self(
            self.0.iter().filter(|s| call.accepts(s)).cloned().collect(),
        ))
    }
    fn member(set: &dyn TypeDefinition, value: &dyn TypeDefinition) -> bool {
        if let Some(set) = set.as_any().downcast_ref::<Candidates>() {
            set.contains(value)
        } else if set.as_any().is::<Never>() {
            false
        } else {
            set.same_type(value)
        }
    }
}

impl PartialEq for OperatorOverloads {
    fn eq(&self, other: &Self) -> bool {
        self.candidates().same_type(other.candidates().as_ref())
    }
}
impl Eq for OperatorOverloads {}

#[cfg(test)]
mod overload_tests {
    use super::{OperatorCall, OperatorOverloads, OperatorSignature};
    use crate::sem::{FullName, OperatorName};
    use crate::types::{Any, Candidates, Never, TypeDefinition, array::ArrayType, base::BaseType};

    #[test]
    fn filtering_keeps_correlated_signatures_order_and_duplicates() {
        let a = BaseType::new(&FullName::from_local_name("a"));
        let b = ArrayType::new(&a);
        let name = OperatorName::from_parts(Some("a.b"), "+");
        let first = OperatorSignature::binary(&name, &a, &b, &a, true);
        let second = OperatorSignature::binary(&name, &b, &a, &b, false);
        let overloads = OperatorOverloads::new(&[first.clone(), second.clone(), first.clone()]);
        assert_eq!(overloads, OperatorOverloads::new(&[second, first.clone()]));
        assert!(
            overloads
                .match_call(&OperatorCall::binary(&a, &a))
                .is_empty()
        );
        let set = Candidates::from_candidates(&[a.clone_box(), b.clone_box()]);
        let matches = overloads.match_call(&OperatorCall::binary(set.as_ref(), set.as_ref()));
        assert_eq!(matches.overloads().signatures(), overloads.signatures());
        let narrowed = matches.filter_operand(0, &a).filter_result(&a);
        assert_eq!(narrowed.overloads().signatures(), &[first.clone(), first]);
        assert!(
            narrowed
                .operand_candidates(1)
                .as_any()
                .downcast_ref::<Candidates>()
                .unwrap()
                .contains(&b)
        );
        assert!(
            narrowed
                .result_candidates()
                .as_any()
                .downcast_ref::<Candidates>()
                .unwrap()
                .contains(&a)
        );
        assert!(narrowed.filter_operand(1, &a).is_empty());
        assert!(matches.filter_operand(9, &a).is_empty());
        assert!(
            matches
                .filter_result(&Never::new())
                .result_candidates()
                .as_any()
                .is::<Never>()
        );
        assert!(matches.filter_operand(0, &Any::new()).is_empty());
        assert!(
            matches.filter_result(&b).overloads().signatures()[0]
                .result()
                .same_type(&b)
        );
        assert!(!overloads.candidates().same_type(&a));
    }
}

/// Whole surviving signatures with their original syntactic operand positions.
/// No reordering or variadic expansion applies to operator invocations.
#[derive(Debug, Clone)]
pub struct OperatorCallMatches(OperatorOverloads);

impl OperatorCallMatches {
    /// Whole surviving declarations, retaining order, duplicates and unary direction.
    pub const fn overloads(&self) -> &OperatorOverloads {
        &self.0
    }
    /// Whether no signature accepts the invocation.
    pub const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    /// Accepted types for a supplied syntactic operand; unary index zero is its value.
    pub fn operand_candidates(&self, index: usize) -> TypeReference {
        self.0.operand_candidates(index)
    }
    /// Possible result types, or `Never` for empty matches.
    pub fn result_candidates(&self) -> TypeReference {
        self.0.result_candidates()
    }
    /// Further constrain an operand while retaining complete signatures.
    pub fn filter_operand(&self, index: usize, candidates: &dyn TypeDefinition) -> Self {
        Self(self.0.filter_operand(index, candidates))
    }
    /// Further constrain results while retaining complete signatures.
    pub fn filter_result(&self, candidates: &dyn TypeDefinition) -> Self {
        Self(self.0.filter_result(candidates))
    }
}

#[cfg(test)]
mod call_matches_tests {
    use super::{OperatorCall, OperatorForm, OperatorOverloads, OperatorSignature};
    use crate::sem::{FullName, OperatorName};
    use crate::types::{Candidates, Never, base::BaseType};

    #[rstest::rstest]
    #[case::binary(OperatorForm::binary())]
    #[case::postfix(OperatorForm::left_unary())]
    #[case::prefix(OperatorForm::right_unary())]
    fn invocation_form_selects_whole_signatures(#[case] form: OperatorForm) {
        let a = BaseType::new(&FullName::from_local_name("a"));
        let name = OperatorName::from_parts(None, "~");
        let signatures = [
            OperatorSignature::binary(&name, &a, &a, &a, true),
            OperatorSignature::left_unary(&name, &a, &a, true),
            OperatorSignature::right_unary(&name, &a, &a, true),
        ];
        let overloads = OperatorOverloads::new(&signatures);
        let call = if form == OperatorForm::binary() {
            OperatorCall::binary(&a, &a)
        } else if form == OperatorForm::left_unary() {
            OperatorCall::left_unary(&a)
        } else {
            OperatorCall::right_unary(&a)
        };
        let matches = overloads.match_call(&call);
        assert_eq!(matches.overloads().signatures().len(), 1);
        let signature = &matches.overloads().signatures()[0];
        assert_eq!(signature.form(), form);
        assert_eq!(signature.left_operand().is_some(), form.has_left());
        assert_eq!(signature.right_operand().is_some(), form.has_right());
        assert!(
            matches
                .operand_candidates(0)
                .as_any()
                .downcast_ref::<Candidates>()
                .unwrap()
                .contains(&a)
        );
        assert_eq!(
            matches.operand_candidates(1).as_any().is::<Never>(),
            form != OperatorForm::binary()
        );
        assert!(matches.operand_candidates(2).as_any().is::<Never>());
        assert!(
            overloads
                .match_call(&OperatorCall::left_unary(&Never::new()))
                .is_empty()
        );
        let empty = Candidates::from_candidates(&[]);
        assert!(
            overloads
                .match_call(&OperatorCall::right_unary(empty.as_ref()))
                .is_empty()
        );
        assert!(
            OperatorOverloads::default()
                .candidates()
                .as_any()
                .is::<Never>()
        );
        assert!(
            OperatorOverloads::default()
                .operand_candidates(0)
                .as_any()
                .is::<Never>()
        );
    }
}
