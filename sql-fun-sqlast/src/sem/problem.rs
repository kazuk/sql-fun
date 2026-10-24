//! SQL analysis diagnostic adapters.
use ::sql_fun_core::analysis_problem as diagnostic;
pub use ::sql_fun_core::analysis_problem::{
    AlterColumnProblem, AnalysisProblem, ArrayProblem, CallArgumentProblem, ColumnAttributeProblem,
    CreationProblem, DeleteProblem, DeleteProblemKind, DomainBaseTypeNameNotFound,
    FunctionNotFound, IndexProblem, IndexProblemKind, InsertProblem, InsertProblemKind,
    MinMaxProblem, NameResolution, OverloadResolution, OwnershipProblem,
    ParameterConstraintProblem, QueryProblem, RuleDefinitionProblem, RuleProblem,
    TableFunctionProblem, TriggerProblem, TruthTestProblem, UpdateProblem, UpdateProblemKind,
    ViewProblem,
};
impl From<&crate::sem::FullName> for diagnostic::FullName {
    fn from(value: &crate::sem::FullName) -> Self {
        Self::new(&value.to_string())
    }
}
impl From<&crate::sem::ColumnName> for diagnostic::ColumnName {
    fn from(value: &crate::sem::ColumnName) -> Self {
        Self::new(&value.to_string())
    }
}
impl From<&crate::sem::TableName> for diagnostic::TableName {
    fn from(value: &crate::sem::TableName) -> Self {
        Self::new(&value.to_string())
    }
}
impl From<&crate::sem::TypeReference> for diagnostic::TypeReference {
    fn from(value: &crate::sem::TypeReference) -> Self {
        Self::new(&if value.is_dynamic() {
            "record".to_owned()
        } else {
            value.to_string()
        })
    }
}
impl From<&crate::sem::TypeDefinition> for diagnostic::TypeDefinition {
    fn from(value: &crate::sem::TypeDefinition) -> Self {
        Self::new(&format!("{value:?}"))
    }
}
impl From<&crate::sem::SemScalarExpr> for diagnostic::SemScalarExpr {
    fn from(value: &crate::sem::SemScalarExpr) -> Self {
        Self::new(&format!("{value:?}"))
    }
}
impl From<crate::sem::SemScalarExpr> for diagnostic::SemScalarExpr {
    fn from(value: crate::sem::SemScalarExpr) -> Self {
        Self::from(&value)
    }
}
impl From<&crate::sem::DefElemKey> for diagnostic::DefElemKey {
    fn from(value: &crate::sem::DefElemKey) -> Self {
        Self::new(&format!("{value:?}"))
    }
}
impl From<&crate::sem::OperatorName> for diagnostic::OperatorName {
    fn from(value: &crate::sem::OperatorName) -> Self {
        Self::new(&format!("{value:?}"))
    }
}
impl From<&crate::sem::type_system::ArgumentBindingCollection>
    for diagnostic::ArgumentBindingCollection
{
    fn from(value: &crate::sem::type_system::ArgumentBindingCollection) -> Self {
        Self::new(&format!("{value:?}"))
    }
}
impl From<&crate::syn::AExprKind> for diagnostic::AExprKind {
    fn from(value: &crate::syn::AExprKind) -> Self {
        Self::new(&format!("{value:?}"))
    }
}
impl From<crate::syn::AExprKind> for diagnostic::AExprKind {
    fn from(value: crate::syn::AExprKind) -> Self {
        Self::from(&value)
    }
}
impl From<&crate::sem::data_source::AliasName> for diagnostic::AliasName {
    fn from(value: &crate::sem::data_source::AliasName) -> Self {
        Self::new(&value.to_string())
    }
}
impl From<&crate::sem::ViewName> for diagnostic::ViewName {
    fn from(value: &crate::sem::ViewName) -> Self {
        Self::new(&value.to_string())
    }
}
impl From<&crate::sem::RuleIdentity> for diagnostic::RuleIdentity {
    fn from(value: &crate::sem::RuleIdentity) -> Self {
        Self::new(&format!("{value:?}"))
    }
}
impl From<&crate::sem::TriggerIdentity> for diagnostic::TriggerIdentity {
    fn from(value: &crate::sem::TriggerIdentity) -> Self {
        Self::new(&format!("{value:?}"))
    }
}
impl From<&crate::sem::OwnershipIdentity> for diagnostic::OwnershipIdentity {
    fn from(value: &crate::sem::OwnershipIdentity) -> Self {
        Self::new(&format!("{value:?}"))
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
impl From<&crate::sem::OperatorDefinition> for diagnostic::OperatorDefinition {
    fn from(value: &crate::sem::OperatorDefinition) -> Self {
        Self::new(&format!("{value:?}"))
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
impl From<&crate::sem::FullName> for diagnostic::DebugName {
    fn from(value: &crate::sem::FullName) -> Self {
        Self::new(&format!("{value:?}"))
    }
}
