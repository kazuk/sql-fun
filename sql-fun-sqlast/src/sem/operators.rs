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
    fn parameter_signature(&self) -> crate::sem::parameter_resolution::Signature {
        crate::sem::parameter_resolution::Signature::new(
            &[self.left_arg_type.clone(), self.right_arg_type.clone()],
            &self.result_type,
        )
    }
}

impl UnaryOperatorDefinition {
    fn parameter_signature(&self) -> crate::sem::parameter_resolution::Signature {
        crate::sem::parameter_resolution::Signature::new(
            std::slice::from_ref(&self.arg_type),
            &self.result_type,
        )
    }
}

impl OperatorDefinition {
    // Instantiate simple array comparisons using the existing catalog type
    // universe, retaining the existing operator signature ranking rules.
    pub(crate) fn comparison_signatures(
        &self,
        context: &impl crate::sem::ParseContext,
    ) -> Vec<crate::sem::parameter_resolution::Signature> {
        let mut signatures = self.parameter_signatures();
        if let Some(overloads) = self.binary_overloads() {
            for overload in overloads {
                if overload.left_arg_type().is_any_array()
                    && overload.right_arg_type().is_any_array()
                {
                    let arrays: std::collections::HashSet<_> = context
                        .type_references()
                        .into_iter()
                        .map(|ty| ty.array_type())
                        .collect();
                    for array in arrays {
                        // Concrete declarations with the same inputs take
                        // precedence over an instantiated polymorphic declaration.
                        if overloads.iter().any(|candidate| {
                            candidate.left_arg_type() == &array
                                && candidate.right_arg_type() == &array
                        }) {
                            continue;
                        }
                        signatures.push(crate::sem::parameter_resolution::Signature::new(
                            &[array.clone(), array],
                            overload.get_return_type(),
                        ));
                    }
                }
            }
        }
        signatures
    }

    pub(crate) fn parameter_signatures(&self) -> Vec<crate::sem::parameter_resolution::Signature> {
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
