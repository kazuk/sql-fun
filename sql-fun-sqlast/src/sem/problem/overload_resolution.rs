use crate::sem::{
    FullName, OperatorDefinition, TypeReference, type_system::ArgumentBindingCollection,
};

#[derive(Debug, Clone)]
pub enum OverloadResolution {
    /// function overload resolve failed
    FunctionOverloadResolutionFailed(FullName, ArgumentBindingCollection),

    /// Binary operator overload resolve failed
    BinaryOperatorOverloadResolutionFailed(OperatorDefinition, TypeReference, TypeReference),

    /// Unary operator overload resolve failed
    UnaryOperatorOverloadResolutionFailed(OperatorDefinition, TypeReference),

    /// Ambiguous operator overload
    BinaryOperatorOverloadAmbiguous(OperatorDefinition, TypeReference, TypeReference),

    /// Unary operator overload ambiguous
    UnaryOperatorOverloadAmbiguous(OperatorDefinition, TypeReference),
}

impl OverloadResolution {
    /// Stable diagnostic classification, independent of formatted debug output.
    #[must_use]
    pub(super) const fn code(&self) -> &'static str {
        match self {
            Self::FunctionOverloadResolutionFailed(..) => "function_overload_resolution_failed",
            Self::BinaryOperatorOverloadResolutionFailed(..) => {
                "binary_operator_overload_resolution_failed"
            }
            Self::UnaryOperatorOverloadResolutionFailed(..) => {
                "unary_operator_overload_resolution_failed"
            }
            Self::BinaryOperatorOverloadAmbiguous(..) => "binary_operator_overload_ambiguous",
            Self::UnaryOperatorOverloadAmbiguous(..) => "unary_operator_overload_ambiguous",
        }
    }

    /// function argument type mismatch to overloads
    pub fn function_overload_resolution_failed(
        func_name: &FullName,
        arg_types: &ArgumentBindingCollection,
    ) -> Self {
        Self::FunctionOverloadResolutionFailed(func_name.clone(), arg_types.to_owned())
    }

    /// Binary operator type not matches overloads
    pub fn binary_operator_overload_resolution_failed(
        operator_def: &OperatorDefinition,
        left_type: &TypeReference,
        right_type: &TypeReference,
    ) -> Self {
        Self::BinaryOperatorOverloadResolutionFailed(
            operator_def.clone(),
            left_type.clone(),
            right_type.clone(),
        )
    }

    /// Unarry operator type not matches overloads
    pub fn unary_operator_overload_resolution_failed(
        operator_def: &OperatorDefinition,
        arg_type: &TypeReference,
    ) -> Self {
        Self::UnaryOperatorOverloadResolutionFailed(operator_def.clone(), arg_type.clone())
    }

    /// Binary operaot type overloads ambiguous
    pub fn binary_operator_overload_ambiguous(
        operator_def: &OperatorDefinition,
        left_type: &TypeReference,
        right_type: &TypeReference,
    ) -> Self {
        Self::BinaryOperatorOverloadAmbiguous(
            operator_def.clone(),
            left_type.clone(),
            right_type.clone(),
        )
    }

    /// Unarray operator type ambiguous
    pub fn unary_operator_overload_ambiguous(
        operator_def: &OperatorDefinition,
        arg_type: &TypeReference,
    ) -> Self {
        Self::UnaryOperatorOverloadAmbiguous(operator_def.clone(), arg_type.clone())
    }
}

impl super::AnalysisProblem {
    /// function argument type mismatch to overloads
    #[tracing::instrument()]
    pub fn function_overload_resolution_failed(
        func_name: &FullName,
        arg_types: &ArgumentBindingCollection,
    ) -> Self {
        Self::OverloadResolution(OverloadResolution::function_overload_resolution_failed(
            func_name, arg_types,
        ))
    }

    /// Binary operator type not matches overloads
    #[tracing::instrument()]
    pub fn binary_operator_overload_resolution_failed(
        operator_def: &OperatorDefinition,
        left_type: &TypeReference,
        right_type: &TypeReference,
    ) -> Self {
        Self::OverloadResolution(
            OverloadResolution::binary_operator_overload_resolution_failed(
                operator_def,
                left_type,
                right_type,
            ),
        )
    }

    /// Unarry operator type not matches overloads
    #[tracing::instrument()]
    pub fn unary_operator_overload_resolution_failed(
        operator_def: &OperatorDefinition,
        arg_type: &TypeReference,
    ) -> Self {
        Self::OverloadResolution(
            OverloadResolution::unary_operator_overload_resolution_failed(operator_def, arg_type),
        )
    }

    /// Binary operaot type overloads ambiguous
    #[tracing::instrument()]
    pub fn binary_operator_overload_ambiguous(
        operator_def: &OperatorDefinition,
        left_type: &TypeReference,
        right_type: &TypeReference,
    ) -> Self {
        Self::OverloadResolution(OverloadResolution::binary_operator_overload_ambiguous(
            operator_def,
            left_type,
            right_type,
        ))
    }

    /// Unarray operator type ambiguous
    #[tracing::instrument()]
    pub fn unary_operator_overload_ambiguous(
        operator_def: &OperatorDefinition,
        arg_type: &TypeReference,
    ) -> Self {
        Self::OverloadResolution(OverloadResolution::unary_operator_overload_ambiguous(
            operator_def,
            arg_type,
        ))
    }
}
