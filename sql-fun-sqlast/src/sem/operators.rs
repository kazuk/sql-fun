use crate::{
    names::OperatorName,
    sem::{
        CascadeRemoval, CascadeTarget, OperatorLinks, TypeReference,
        parameter_resolution::Signature,
    },
};

type RegisteredRemovals = Vec<Box<dyn CascadeRemoval>>;

// Catalog dependents do not participate in the identity of a bound expression.
#[derive(Debug, Clone, Default)]
pub(crate) struct OperatorDependents(Option<std::sync::Arc<RegisteredRemovals>>);

impl PartialEq for OperatorDependents {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl Eq for OperatorDependents {}

impl std::hash::Hash for OperatorDependents {
    fn hash<H: std::hash::Hasher>(&self, _state: &mut H) {}
}

impl OperatorDependents {
    pub(crate) fn entries(&self) -> &[Box<dyn CascadeRemoval>] {
        match &self.0 {
            Some(entries) => entries.as_slice(),
            None => &[],
        }
    }

    pub(crate) fn replace(&mut self, target: &CascadeTarget, removal: Option<&dyn CascadeRemoval>) {
        if let Some(entries) = &mut self.0 {
            std::sync::Arc::make_mut(entries).retain(|action| &action.target() != target);
        }
        if let Some(removal) = removal {
            let entries = self.0.get_or_insert_with(Default::default);
            std::sync::Arc::make_mut(entries).push(removal.clone_box());
        }
    }
}

/// Binary operators
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct BinaryOperatorDefinition {
    left_arg_type: TypeReference,
    right_arg_type: TypeReference,
    result_type: TypeReference,
    strict: bool,
    #[serde(skip)]
    settings: Option<Box<OperatorLinks>>,
    #[serde(skip)]
    dependents: OperatorDependents,
}

impl BinaryOperatorDefinition {
    pub(crate) const fn dependents(&self) -> &OperatorDependents {
        &self.dependents
    }

    pub(crate) const fn dependents_mut(&mut self) -> &mut OperatorDependents {
        &mut self.dependents
    }

    pub(crate) fn with_settings(mut self, settings: Option<OperatorLinks>) -> Self {
        self.settings = settings.map(Box::new);
        self
    }

    pub(crate) fn settings(&self) -> Option<&OperatorLinks> {
        self.settings.as_deref()
    }

    fn diagnostic_signature(&self) -> ::sql_fun_core::analysis_problem::OperatorSignature {
        ::sql_fun_core::analysis_problem::OperatorSignature::new(
            &[(&self.left_arg_type).into(), (&self.right_arg_type).into()],
            &(&self.result_type).into(),
            self.strict,
        )
    }

    /// create instance
    #[must_use]
    pub fn new(
        left_arg_type: &TypeReference,
        right_arg_type: &TypeReference,
        result_type: &TypeReference,
        strict: bool,
    ) -> Self {
        Self {
            left_arg_type: left_arg_type.clone(),
            right_arg_type: right_arg_type.clone(),
            result_type: result_type.clone(),
            strict,
            settings: None,
            dependents: OperatorDependents::default(),
        }
    }

    /// operator return type
    #[must_use]
    pub const fn get_return_type(&self) -> &TypeReference {
        &self.result_type
    }

    /// strict
    #[must_use]
    pub const fn is_strict(&self) -> bool {
        self.strict
    }

    /// left argument type
    #[must_use]
    pub const fn left_arg_type(&self) -> &TypeReference {
        &self.left_arg_type
    }

    /// right argument type
    #[must_use]
    pub const fn right_arg_type(&self) -> &TypeReference {
        &self.right_arg_type
    }
}

/// unary operator definitions
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct UnaryOperatorDefinition {
    arg_type: TypeReference,
    result_type: TypeReference,
    strict: bool,
    #[serde(skip)]
    settings: Option<Box<OperatorLinks>>,
    #[serde(skip)]
    dependents: OperatorDependents,
}

impl UnaryOperatorDefinition {
    pub(crate) const fn dependents(&self) -> &OperatorDependents {
        &self.dependents
    }

    pub(crate) const fn dependents_mut(&mut self) -> &mut OperatorDependents {
        &mut self.dependents
    }

    pub(crate) fn with_settings(mut self, settings: Option<OperatorLinks>) -> Self {
        self.settings = settings.map(Box::new);
        self
    }

    pub(crate) fn settings(&self) -> Option<&OperatorLinks> {
        self.settings.as_deref()
    }

    fn diagnostic_signature(&self) -> ::sql_fun_core::analysis_problem::OperatorSignature {
        ::sql_fun_core::analysis_problem::OperatorSignature::new(
            &[(&self.arg_type).into()],
            &(&self.result_type).into(),
            self.strict,
        )
    }

    /// create instance
    #[must_use]
    pub fn new(arg_type: &TypeReference, result_type: &TypeReference, strict: bool) -> Self {
        Self {
            arg_type: arg_type.clone(),
            result_type: result_type.clone(),
            strict,
            settings: None,
            dependents: OperatorDependents::default(),
        }
    }

    /// return type of operator
    #[must_use]
    pub const fn get_return_type(&self) -> &TypeReference {
        &self.result_type
    }

    /// strict
    #[must_use]
    pub const fn is_strict(&self) -> bool {
        self.strict
    }

    /// argument type
    #[must_use]
    pub const fn arg_type(&self) -> &TypeReference {
        &self.arg_type
    }
}

/// Binary / Unary operator
#[derive(Debug, Clone)]
pub enum OperatorDefinition {
    /// binary operator
    Binary(Vec<BinaryOperatorDefinition>),
    /// left assoc unary
    LeftUnary(Vec<UnaryOperatorDefinition>),
    /// right assoc unary
    RightUnary(Vec<UnaryOperatorDefinition>),
}

impl OperatorDefinition {
    pub(crate) const fn is_empty(&self) -> bool {
        match self {
            Self::Binary(entries) => entries.is_empty(),
            Self::LeftUnary(entries) | Self::RightUnary(entries) => entries.is_empty(),
        }
    }

    pub(crate) fn has_operands(
        &self,
        left: Option<&TypeReference>,
        right: Option<&TypeReference>,
    ) -> bool {
        match (self, left, right) {
            (Self::Binary(overloads), Some(left), Some(right)) => {
                overloads.iter().any(|candidate| {
                    candidate.left_arg_type() == left && candidate.right_arg_type() == right
                })
            }
            (Self::LeftUnary(overloads), Some(arg), None)
            | (Self::RightUnary(overloads), None, Some(arg)) => overloads
                .iter()
                .any(|candidate| candidate.arg_type() == arg),
            _ => false,
        }
    }

    pub(crate) fn diagnostic_definition(
        &self,
        name: &OperatorName,
    ) -> ::sql_fun_core::analysis_problem::OperatorDefinition {
        let (left_operand, right_operand) = match self {
            Self::Binary(_) => (true, true),
            Self::LeftUnary(_) => (true, false),
            Self::RightUnary(_) => (false, true),
        };
        ::sql_fun_core::analysis_problem::OperatorDefinition::new(
            name,
            &self.diagnostic_signatures(),
            left_operand,
            right_operand,
        )
    }

    fn diagnostic_signatures(&self) -> Vec<::sql_fun_core::analysis_problem::OperatorSignature> {
        match self {
            Self::Binary(overloads) => overloads
                .iter()
                .map(BinaryOperatorDefinition::diagnostic_signature)
                .collect(),
            Self::LeftUnary(overloads) | Self::RightUnary(overloads) => overloads
                .iter()
                .map(UnaryOperatorDefinition::diagnostic_signature)
                .collect(),
        }
    }

    pub(crate) const fn binary_overloads(&self) -> Option<&[BinaryOperatorDefinition]> {
        match self {
            Self::Binary(overloads) => Some(overloads.as_slice()),
            _ => None,
        }
    }

    /// binary
    #[must_use]
    pub fn binary(binaries: &[BinaryOperatorDefinition]) -> Self {
        Self::Binary(Vec::from_iter(binaries.to_vec()))
    }

    /// left assoc unary
    #[must_use]
    pub fn left_unary(unaries: &[UnaryOperatorDefinition]) -> Self {
        Self::LeftUnary(Vec::from_iter(unaries.to_vec()))
    }

    /// right assoc unary
    #[must_use]
    pub fn right_unary(unaries: &[UnaryOperatorDefinition]) -> Self {
        Self::RightUnary(Vec::from_iter(unaries.to_vec()))
    }
}

impl BinaryOperatorDefinition {
    fn parameter_signature(&self) -> Signature {
        Signature::new(
            &[self.left_arg_type.clone(), self.right_arg_type.clone()],
            &self.result_type,
        )
    }
}

impl UnaryOperatorDefinition {
    fn parameter_signature(&self) -> Signature {
        Signature::new(std::slice::from_ref(&self.arg_type), &self.result_type)
    }
}

impl OperatorDefinition {
    pub(crate) fn parameter_signatures(&self) -> Vec<Signature> {
        match self {
            Self::Binary(overloads) => overloads
                .iter()
                .map(BinaryOperatorDefinition::parameter_signature)
                .collect(),
            Self::LeftUnary(overloads) | Self::RightUnary(overloads) => overloads
                .iter()
                .map(UnaryOperatorDefinition::parameter_signature)
                .collect(),
        }
    }
}
