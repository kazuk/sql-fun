use crate::{
    names::OperatorName,
    sem::{TypeReference, parameter_resolution::Signature},
};

/// Binary operators
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct BinaryOperatorDefinition {
    left_arg_type: TypeReference,
    right_arg_type: TypeReference,
    result_type: TypeReference,
    strict: bool,
}

impl BinaryOperatorDefinition {
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
}

impl UnaryOperatorDefinition {
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
