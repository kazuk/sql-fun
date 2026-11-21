use crate::analysis_problem::{DiagnosticMessage, FullName, TypeReference};

/// Role of a routine in a foreign-data wrapper declaration.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, ::serde::Serialize, ::serde::Deserialize)]
#[cfg_attr(feature = "openrpc-schema", derive(::schemars::JsonSchema))]
pub enum FdwRoutineKind {
    /// Zero-input handler.
    Handler,
    /// Option validator.
    Validator,
}

impl FdwRoutineKind {
    const fn label(self) -> &'static str {
        match self {
            Self::Handler => "handler",
            Self::Validator => "validator",
        }
    }

    const fn signature(self) -> &'static str {
        match self {
            Self::Handler => "",
            Self::Validator => "pg_catalog.text[], pg_catalog.oid",
        }
    }
}

/// Facts explaining why a foreign-data wrapper declaration was rejected.
#[derive(Clone, Debug, PartialEq, Eq, ::serde::Serialize, ::serde::Deserialize)]
#[cfg_attr(feature = "openrpc-schema", derive(::schemars::JsonSchema))]
pub enum ForeignDataWrapperFailure {
    /// The database-wide wrapper name already exists.
    DuplicateWrapper,
    /// The named option appears more than once.
    DuplicateOption(String),
    /// More than one clause specifies the same routine role.
    ConflictingClause(FdwRoutineKind),
    /// The routine identifier components cannot be resolved in the selected database.
    InvalidRoutineName(FdwRoutineKind, Vec<String>),
    /// No visible function matches the requested name and exact signature.
    RoutineNotFound(FdwRoutineKind, FullName),
    /// The resolved handler has an incompatible or unknown return type.
    InvalidHandlerReturn(FullName, Option<TypeReference>),
}

/// Structured validation failure for a named foreign-data wrapper.
#[derive(Clone, Debug, PartialEq, Eq, ::serde::Serialize, ::serde::Deserialize)]
#[cfg_attr(feature = "openrpc-schema", derive(::schemars::JsonSchema))]
pub struct ForeignDataWrapperProblem {
    wrapper: String,
    failure: ForeignDataWrapperFailure,
}

impl ForeignDataWrapperProblem {
    /// Associate the failure with the declaration's logical wrapper name.
    #[must_use]
    pub fn new(wrapper: &str, failure: &ForeignDataWrapperFailure) -> Self {
        Self {
            wrapper: wrapper.to_owned(),
            failure: failure.clone(),
        }
    }

    /// Database-wide wrapper name.
    #[must_use]
    pub fn wrapper(&self) -> &str {
        &self.wrapper
    }

    /// The recorded validation failure and its affected identity.
    #[must_use]
    pub const fn failure(&self) -> &ForeignDataWrapperFailure {
        &self.failure
    }
}

impl DiagnosticMessage for ForeignDataWrapperProblem {
    fn diagnostic_message(&self) -> String {
        match &self.failure {
            ForeignDataWrapperFailure::DuplicateWrapper => {
                format!("foreign-data wrapper {:?} already exists", self.wrapper)
            }
            ForeignDataWrapperFailure::DuplicateOption(option) => {
                format!("foreign-data wrapper option {option:?} provided more than once")
            }
            ForeignDataWrapperFailure::ConflictingClause(kind) => {
                format!("conflicting foreign-data wrapper {} clauses", kind.label())
            }
            ForeignDataWrapperFailure::InvalidRoutineName(kind, parts) => format!(
                "cannot resolve foreign-data wrapper {} name {} in the selected database",
                kind.label(),
                parts.join("."),
            ),
            ForeignDataWrapperFailure::RoutineNotFound(kind, name) => format!(
                "foreign-data wrapper {} requires a function matching {name}({})",
                kind.label(),
                kind.signature(),
            ),
            ForeignDataWrapperFailure::InvalidHandlerReturn(name, _) => {
                format!("foreign-data wrapper handler {name} must return pg_catalog.fdw_handler")
            }
        }
    }
}
