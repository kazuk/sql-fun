use crate::analysis_problem::{DiagnosticMessage, DiagnosticMetadata};

/// A CHECK definition that cannot be applied to the analysis catalog.
#[derive(Debug, Clone, ::serde::Serialize, ::serde::Deserialize)]
#[cfg_attr(feature = "openrpc-schema", derive(::schemars::JsonSchema))]
pub enum CheckProblem {
    /// The expression cannot be coerced to boolean. Diagnostic code: `check_boolean_type`.
    BooleanType,
    /// An unknown string is not valid boolean input. Diagnostic code: `check_boolean_literal`.
    BooleanLiteral(String),
    /// CHECK cannot contain a subquery, aggregate, window or set-returning expression. Diagnostic code: `check_expression_placement`.
    ExpressionPlacement,
    /// CHECK cannot contain a bind parameter. Diagnostic code: `check_parameter`.
    Parameter,
    /// CHECK constraints cannot be deferred. Diagnostic code: `check_deferrability`.
    Deferrability,
    /// An explicit constraint name is already used on this table. Diagnostic code: `check_duplicate_name`.
    DuplicateName(String),
}
impl CheckProblem {
    pub(super) const fn code(&self) -> &'static str {
        match self {
            Self::BooleanType => "check_boolean_type",
            Self::BooleanLiteral(_) => "check_boolean_literal",
            Self::ExpressionPlacement => "check_expression_placement",
            Self::Parameter => "check_parameter",
            Self::Deferrability => "check_deferrability",
            Self::DuplicateName(_) => "check_duplicate_name",
        }
    }
    pub(super) fn render_metadata(&self) -> DiagnosticMetadata {
        DiagnosticMetadata::from(self.diagnostic_message())
    }
}
#[cfg(test)]
mod diagnostic_code_tests {
    use crate::analysis_problem::{AnalysisProblem, CheckProblem};

    #[rstest::rstest]
    #[case::boolean_type(CheckProblem::BooleanType, "check_boolean_type")]
    #[case::boolean_literal(CheckProblem::BooleanLiteral("bad".into()), "check_boolean_literal")]
    #[case::placement(CheckProblem::ExpressionPlacement, "check_expression_placement")]
    #[case::parameter(CheckProblem::Parameter, "check_parameter")]
    #[case::deferrability(CheckProblem::Deferrability, "check_deferrability")]
    #[case::duplicate(CheckProblem::DuplicateName("repeated".into()), "check_duplicate_name")]
    fn variant_codes_survive_structured_diagnostic_round_trips(
        #[case] problem: CheckProblem,
        #[case] expected: &str,
    ) -> testresult::TestResult {
        let diagnostic = AnalysisProblem::Check(problem);
        assert_eq!(diagnostic.code(), expected);
        let restored: AnalysisProblem =
            ::serde_json::from_str(&::serde_json::to_string(&diagnostic)?)?;
        assert_eq!(restored.code(), expected);
        Ok(())
    }
}

impl DiagnosticMessage for CheckProblem {
    fn diagnostic_message(&self) -> String {
        match self {
            Self::BooleanType => "CHECK expression must have boolean type".into(),
            Self::BooleanLiteral(value) => format!("invalid CHECK boolean input: {value}"),
            Self::ExpressionPlacement => {
                "CHECK cannot contain subqueries, aggregates, windows or set-returning expressions"
                    .into()
            }
            Self::Parameter => "CHECK cannot contain bind parameters".into(),
            Self::Deferrability => "CHECK constraints cannot be deferred".into(),
            Self::DuplicateName(name) => {
                format!("constraint name already exists on this table: {name}")
            }
        }
    }
}
