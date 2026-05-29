use crate::sem::TypeReference;

/// Binary operators
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct BinaryOperatorDefinition {
    left_arg_type: TypeReference,
    right_arg_type: TypeReference,
    result_type: TypeReference,
    strict: bool,
}

impl BinaryOperatorDefinition {
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
    pub fn get_return_type(&self) -> &TypeReference {
        &self.result_type
    }

    /// strict
    #[must_use]
    pub fn is_strict(&self) -> bool {
        self.strict
    }

    /// left argument type
    #[must_use]
    pub fn left_arg_type(&self) -> &TypeReference {
        &self.left_arg_type
    }

    /// right argument type
    #[must_use]
    pub fn right_arg_type(&self) -> &TypeReference {
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
    pub fn get_return_type(&self) -> &TypeReference {
        &self.result_type
    }

    /// strict
    #[must_use]
    pub fn is_strict(&self) -> bool {
        self.strict
    }

    /// argument type
    #[must_use]
    pub fn arg_type(&self) -> &TypeReference {
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
