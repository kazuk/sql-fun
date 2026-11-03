//! SQL analysis diagnostic adapters.
use ::sql_fun_core::analysis_problem as diagnostic;
pub use ::sql_fun_core::analysis_problem::{
    AlterColumnProblem, AnalysisProblem, ArrayProblem, CallArgumentProblem, CollateProblem,
    ColumnAttributeProblem, CreationProblem, DeleteProblem, DeleteProblemKind, DistinctProblem,
    DomainBaseTypeNameNotFound, FunctionNotFound, IndexProblem, IndexProblemKind, InsertProblem,
    InsertProblemKind, MembershipProblem, MinMaxProblem, NameResolution, OverloadResolution,
    OwnershipProblem, ParameterConstraintProblem, QueryProblem, RuleDefinitionProblem, RuleProblem,
    TableFunctionProblem, TableSampleProblem, TriggerProblem, TruthTestProblem, UpdateProblem,
    UpdateProblemKind, ViewProblem,
};
impl From<&crate::names::FullName> for diagnostic::FullName {
    fn from(value: &crate::names::FullName) -> Self {
        Self::new(&value.to_string())
    }
}
impl From<&crate::names::ColumnName> for diagnostic::ColumnName {
    fn from(value: &crate::names::ColumnName) -> Self {
        Self::new(&value.to_string())
    }
}
impl From<&crate::names::TableName> for diagnostic::TableName {
    fn from(value: &crate::names::TableName) -> Self {
        Self::new(&value.to_string())
    }
}
impl From<&crate::sem::TypeReference> for diagnostic::TypeReference {
    fn from(value: &crate::sem::TypeReference) -> Self {
        Self::new(&value.to_string())
    }
}
impl From<&crate::sem::TypeDefinition> for diagnostic::TypeDefinition {
    fn from(value: &crate::sem::TypeDefinition) -> Self {
        Self::new(&value.to_string())
    }
}
impl From<&crate::sem::SemScalarExpr> for diagnostic::SemScalarExpr {
    fn from(value: &crate::sem::SemScalarExpr) -> Self {
        Self::new(
            value.diagnostic_kind(),
            value
                .get_type()
                .as_ref()
                .map(diagnostic::TypeReference::from)
                .as_ref(),
            value.is_not_null(),
        )
        .with_identity(value.diagnostic_identity().as_deref())
    }
}
impl From<crate::sem::SemScalarExpr> for diagnostic::SemScalarExpr {
    fn from(value: crate::sem::SemScalarExpr) -> Self {
        Self::from(&value)
    }
}
#[cfg(test)]
mod expression_conversion_tests {
    use super::diagnostic;
    use crate::sem::{InputTypeConstraint, PgBuiltInType, SemScalarExpr};

    #[rstest::rstest]
    #[case::unknown(InputTypeConstraint::any(), None)]
    #[case::known(InputTypeConstraint::types([PgBuiltInType::int4()]), Some("pg_catalog.int4"))]
    fn retains_parameter_identity_and_known_type(
        #[case] constraint: InputTypeConstraint,
        #[case] expected: Option<&str>,
    ) {
        let expression = SemScalarExpr::parameter(2, 0, &constraint);
        let facts = diagnostic::SemScalarExpr::from(&expression);
        assert_eq!(facts.kind(), "parameter");
        assert_eq!(facts.identity(), Some("$2"));
        assert_eq!(
            facts
                .type_reference()
                .map(diagnostic::TypeReference::as_str),
            expected
        );
        assert_eq!(facts.not_null(), None);
    }
}

impl From<&crate::sem::DefElemKey> for diagnostic::DefElemKey {
    fn from(value: &crate::sem::DefElemKey) -> Self {
        Self::new(&value.to_string())
    }
}
impl From<&crate::names::OperatorName> for diagnostic::OperatorName {
    fn from(value: &crate::names::OperatorName) -> Self {
        Self::new(&value.to_string())
    }
}
impl From<&crate::sem::type_system::ArgumentBindingCollection>
    for diagnostic::ArgumentBindingCollection
{
    fn from(value: &crate::sem::type_system::ArgumentBindingCollection) -> Self {
        Self::new(
            &value
                .types()
                .map(|ty| ty.map(diagnostic::TypeReference::from))
                .collect::<Vec<_>>(),
        )
    }
}
impl From<&crate::syn::AExprKind> for diagnostic::AExprKind {
    fn from(value: &crate::syn::AExprKind) -> Self {
        use crate::syn::AExprKind as Kind;
        let description = match value {
            Kind::AexprOp => "operator",
            Kind::AexprOpAny => "ANY comparison",
            Kind::AexprOpAll => "ALL comparison",
            Kind::AexprDistinct => "IS DISTINCT FROM",
            Kind::AexprNotDistinct => "IS NOT DISTINCT FROM",
            Kind::AexprNullif => "NULLIF",
            Kind::AexprIn => "IN",
            Kind::AexprLike => "LIKE",
            Kind::AexprIlike => "ILIKE",
            Kind::AexprSimilar => "SIMILAR TO",
            Kind::AexprBetween => "BETWEEN",
            Kind::AexprNotBetween => "NOT BETWEEN",
            Kind::AexprBetweenSym => "BETWEEN SYMMETRIC",
            Kind::AexprNotBetweenSym => "NOT BETWEEN SYMMETRIC",
            Kind::Undefined => "unrecognized operator expression",
            Kind::OptionNone => "unspecified operator expression",
        };
        Self::new(description)
    }
}
impl From<crate::syn::AExprKind> for diagnostic::AExprKind {
    fn from(value: crate::syn::AExprKind) -> Self {
        Self::from(&value)
    }
}
impl From<&crate::names::AliasName> for diagnostic::AliasName {
    fn from(value: &crate::names::AliasName) -> Self {
        Self::new(&value.to_string())
    }
}
impl From<&crate::names::ViewName> for diagnostic::ViewName {
    fn from(value: &crate::names::ViewName) -> Self {
        Self::new(&value.to_string())
    }
}
impl From<&crate::sem::RuleIdentity> for diagnostic::RuleIdentity {
    fn from(value: &crate::sem::RuleIdentity) -> Self {
        Self::new(&value.to_string())
    }
}
impl From<&crate::sem::TriggerIdentity> for diagnostic::TriggerIdentity {
    fn from(value: &crate::sem::TriggerIdentity) -> Self {
        Self::new(&value.to_string())
    }
}
impl From<&crate::sem::OwnershipIdentity> for diagnostic::OwnershipIdentity {
    fn from(value: &crate::sem::OwnershipIdentity) -> Self {
        Self::new(&value.to_string())
    }
}
impl From<&crate::sem::OwnershipLookup> for diagnostic::OwnershipLookup {
    fn from(value: &crate::sem::OwnershipLookup) -> Self {
        Self::new(value.code(), &value.diagnostic_message())
    }
}
impl From<&crate::sem::Kind> for diagnostic::Kind {
    fn from(value: &crate::sem::Kind) -> Self {
        Self::new(&value.to_string())
    }
}
impl From<crate::sem::Kind> for diagnostic::Kind {
    fn from(value: crate::sem::Kind) -> Self {
        Self::from(&value)
    }
}
impl From<&crate::syn::TypeName> for diagnostic::TypeName {
    fn from(value: &crate::syn::TypeName) -> Self {
        Self::new(&value.full_name().to_string())
    }
}
impl From<&crate::StringSpan> for diagnostic::StringSpan {
    fn from(value: &crate::StringSpan) -> Self {
        Self::new(value.offset(), value.len())
    }
}
impl From<&crate::names::FullName> for diagnostic::DebugName {
    fn from(value: &crate::names::FullName) -> Self {
        Self::new(&value.to_string())
    }
}
