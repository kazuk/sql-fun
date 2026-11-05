use crate::analysis_problem::{
    AnalysisProblem, ArgumentBindingCollection, DiagnosticMessage, DiagnosticMessageFragment,
    FullName, OperatorDefinition, TypeReference, fragments_text,
};

#[derive(Debug, Clone, ::serde::Serialize, ::serde::Deserialize)]
#[cfg_attr(feature = "openrpc-schema", derive(::schemars::JsonSchema))]
/// Function and operator overload failures.
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

impl DiagnosticMessage for OverloadResolution {
    fn diagnostic_message(&self) -> String {
        match self {
            Self::FunctionOverloadResolutionFailed(..) => {
                fragments_text(&self.diagnostic_fragments())
            }
            Self::BinaryOperatorOverloadResolutionFailed(operator, left, right) => {
                format!("no matching overload for operator {operator} with {left} and {right}")
            }
            Self::UnaryOperatorOverloadResolutionFailed(operator, ty) => {
                format!("no matching overload for unary operator {operator} with {ty}")
            }
            Self::BinaryOperatorOverloadAmbiguous(operator, left, right) => {
                format!("ambiguous overload for operator {operator} with {left} and {right}")
            }
            Self::UnaryOperatorOverloadAmbiguous(operator, ty) => {
                format!("ambiguous overload for unary operator {operator} with {ty}")
            }
        }
    }

    fn diagnostic_fragments(&self) -> Vec<DiagnosticMessageFragment<'_>> {
        match self {
            Self::FunctionOverloadResolutionFailed(name, args) => vec![
                "no matching overload for function ".into(),
                name.into(),
                format!(" with {args}").into(),
            ],
            _ => vec![self.diagnostic_message().into()],
        }
    }
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
    #[must_use]
    pub fn function_overload_resolution_failed(
        func_name: impl Into<FullName>,
        arg_types: impl Into<ArgumentBindingCollection>,
    ) -> Self {
        Self::FunctionOverloadResolutionFailed(func_name.into(), arg_types.into())
    }

    /// Binary operator type not matches overloads
    #[must_use]
    pub fn binary_operator_overload_resolution_failed(
        operator_def: impl Into<OperatorDefinition>,
        left_type: impl Into<TypeReference>,
        right_type: impl Into<TypeReference>,
    ) -> Self {
        Self::BinaryOperatorOverloadResolutionFailed(
            operator_def.into(),
            left_type.into(),
            right_type.into(),
        )
    }

    /// Unarry operator type not matches overloads
    #[must_use]
    pub fn unary_operator_overload_resolution_failed(
        operator_def: impl Into<OperatorDefinition>,
        arg_type: impl Into<TypeReference>,
    ) -> Self {
        Self::UnaryOperatorOverloadResolutionFailed(operator_def.into(), arg_type.into())
    }

    /// Binary operaot type overloads ambiguous
    #[must_use]
    pub fn binary_operator_overload_ambiguous(
        operator_def: impl Into<OperatorDefinition>,
        left_type: impl Into<TypeReference>,
        right_type: impl Into<TypeReference>,
    ) -> Self {
        Self::BinaryOperatorOverloadAmbiguous(
            operator_def.into(),
            left_type.into(),
            right_type.into(),
        )
    }

    /// Unarray operator type ambiguous
    #[must_use]
    pub fn unary_operator_overload_ambiguous(
        operator_def: impl Into<OperatorDefinition>,
        arg_type: impl Into<TypeReference>,
    ) -> Self {
        Self::UnaryOperatorOverloadAmbiguous(operator_def.into(), arg_type.into())
    }
}

impl AnalysisProblem {
    /// function argument type mismatch to overloads
    #[must_use]
    pub fn function_overload_resolution_failed(
        func_name: impl Into<FullName>,
        arg_types: impl Into<ArgumentBindingCollection>,
    ) -> Self {
        Self::OverloadResolution(OverloadResolution::function_overload_resolution_failed(
            func_name, arg_types,
        ))
    }

    /// Binary operator type not matches overloads
    #[must_use]
    pub fn binary_operator_overload_resolution_failed(
        operator_def: impl Into<OperatorDefinition>,
        left_type: impl Into<TypeReference>,
        right_type: impl Into<TypeReference>,
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
    #[must_use]
    pub fn unary_operator_overload_resolution_failed(
        operator_def: impl Into<OperatorDefinition>,
        arg_type: impl Into<TypeReference>,
    ) -> Self {
        Self::OverloadResolution(
            OverloadResolution::unary_operator_overload_resolution_failed(operator_def, arg_type),
        )
    }

    /// Binary operaot type overloads ambiguous
    #[must_use]
    pub fn binary_operator_overload_ambiguous(
        operator_def: impl Into<OperatorDefinition>,
        left_type: impl Into<TypeReference>,
        right_type: impl Into<TypeReference>,
    ) -> Self {
        Self::OverloadResolution(OverloadResolution::binary_operator_overload_ambiguous(
            operator_def,
            left_type,
            right_type,
        ))
    }

    /// Unarray operator type ambiguous
    #[must_use]
    pub fn unary_operator_overload_ambiguous(
        operator_def: impl Into<OperatorDefinition>,
        arg_type: impl Into<TypeReference>,
    ) -> Self {
        Self::OverloadResolution(OverloadResolution::unary_operator_overload_ambiguous(
            operator_def,
            arg_type,
        ))
    }
}
