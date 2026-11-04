//! SQL analysis diagnostic adapters.

pub use ::sql_fun_core::analysis_problem::{
    AlterColumnProblem, AnalysisProblem, ArrayProblem, CallArgumentProblem, CheckProblem,
    CollateProblem, ColumnAttributeProblem, CreationProblem, DeleteProblem, DeleteProblemKind,
    DistinctProblem, DomainBaseTypeNameNotFound, FunctionNotFound, IndexProblem, IndexProblemKind,
    InsertProblem, InsertProblemKind, MembershipProblem, MinMaxProblem, NameResolution,
    OverloadResolution, OwnershipProblem, ParameterConstraintProblem, QueryProblem,
    RuleDefinitionProblem, RuleProblem, TableFunctionProblem, TableSampleProblem, TriggerProblem,
    TruthTestProblem, UpdateProblem, UpdateProblemKind, ViewProblem,
};

use crate::{
    StringSpan,
    names::{AliasName, ColumnName, FullName, OperatorName, TableName, ViewName},
    sem::{
        DefElemKey, Kind as SemKind, OwnershipIdentity, OwnershipLookup, RuleIdentity,
        SemScalarExpr as SemSemScalarExpr, TriggerIdentity, TypeDefinition,
        TypeReference as SemTypeReference, type_system::ArgumentBindingCollection,
    },
    syn::{AExprKind, TypeName},
};
use ::sql_fun_core::analysis_problem as diagnostic;

impl From<&FullName> for diagnostic::FullName {
    fn from(value: &FullName) -> Self {
        Self::new(&value.to_string())
    }
}
impl From<&ColumnName> for diagnostic::ColumnName {
    fn from(value: &ColumnName) -> Self {
        Self::new(&value.to_string())
    }
}
impl From<&TableName> for diagnostic::TableName {
    fn from(value: &TableName) -> Self {
        Self::new(&value.to_string())
    }
}
impl From<&SemTypeReference> for diagnostic::TypeReference {
    fn from(value: &SemTypeReference) -> Self {
        Self::new(&value.to_string())
    }
}
impl From<&TypeDefinition> for diagnostic::TypeDefinition {
    fn from(value: &TypeDefinition) -> Self {
        Self::new(&value.to_string())
    }
}
impl From<&SemSemScalarExpr> for diagnostic::SemScalarExpr {
    fn from(value: &SemSemScalarExpr) -> Self {
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
impl From<SemSemScalarExpr> for diagnostic::SemScalarExpr {
    fn from(value: SemSemScalarExpr) -> Self {
        Self::from(&value)
    }
}
#[cfg(test)]
mod expression_conversion_tests {
    use crate::sem::{InputTypeConstraint, PgBuiltInType, SemScalarExpr, problem::diagnostic};

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

impl From<&DefElemKey> for diagnostic::DefElemKey {
    fn from(value: &DefElemKey) -> Self {
        Self::new(&value.to_string())
    }
}
impl From<&OperatorName> for diagnostic::OperatorName {
    fn from(value: &OperatorName) -> Self {
        Self::new(&value.to_string())
    }
}
impl From<&ArgumentBindingCollection> for diagnostic::ArgumentBindingCollection {
    fn from(value: &ArgumentBindingCollection) -> Self {
        Self::new(
            &value
                .types()
                .map(|ty| ty.map(diagnostic::TypeReference::from))
                .collect::<Vec<_>>(),
        )
    }
}
impl From<&AExprKind> for diagnostic::AExprKind {
    fn from(value: &AExprKind) -> Self {
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
impl From<AExprKind> for diagnostic::AExprKind {
    fn from(value: AExprKind) -> Self {
        Self::from(&value)
    }
}
impl From<&AliasName> for diagnostic::AliasName {
    fn from(value: &AliasName) -> Self {
        Self::new(&value.to_string())
    }
}
impl From<&ViewName> for diagnostic::ViewName {
    fn from(value: &ViewName) -> Self {
        Self::new(&value.to_string())
    }
}
impl From<&RuleIdentity> for diagnostic::RuleIdentity {
    fn from(value: &RuleIdentity) -> Self {
        Self::new(&value.to_string())
    }
}
impl From<&TriggerIdentity> for diagnostic::TriggerIdentity {
    fn from(value: &TriggerIdentity) -> Self {
        Self::new(&value.to_string())
    }
}
impl From<&OwnershipIdentity> for diagnostic::OwnershipIdentity {
    fn from(value: &OwnershipIdentity) -> Self {
        Self::new(&value.to_string())
    }
}
impl From<&OwnershipLookup> for diagnostic::OwnershipLookup {
    fn from(value: &OwnershipLookup) -> Self {
        Self::new(value.code(), &value.diagnostic_message())
    }
}
impl From<&SemKind> for diagnostic::Kind {
    fn from(value: &SemKind) -> Self {
        Self::new(&value.to_string())
    }
}
impl From<SemKind> for diagnostic::Kind {
    fn from(value: SemKind) -> Self {
        Self::from(&value)
    }
}
impl From<&TypeName> for diagnostic::TypeName {
    fn from(value: &TypeName) -> Self {
        Self::new(&value.full_name().to_string())
    }
}
impl From<&StringSpan> for diagnostic::StringSpan {
    fn from(value: &StringSpan) -> Self {
        Self::new(value.offset(), value.len())
    }
}
impl From<&FullName> for diagnostic::DebugName {
    fn from(value: &FullName) -> Self {
        Self::new(&value.to_string())
    }
}
