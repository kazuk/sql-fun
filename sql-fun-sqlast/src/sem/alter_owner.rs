use crate::{
    ast::TryFromWithTokens,
    sem::{AnalysisError, AstAndContextPair, OwnerSpecification, OwnershipIdentity, ParseContext},
};

/// Completed target lookup for an ALTER OWNER statement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OwnershipLookup {
    /// A unique, existing typed catalog identity.
    Resolved(OwnershipIdentity),
    /// No definition matches the supplied identity/signature.
    NotFound(OwnershipIdentity),
    /// An omitted signature identifies multiple definitions.
    Ambiguous(Vec<OwnershipIdentity>),
    /// The target kind is incompatible with the existing definition.
    InvalidKind(OwnershipIdentity),
    /// An argument type could not be resolved.
    ArgumentTypeNotFound(OwnershipArgumentFailure),
}
impl OwnershipLookup {
    pub(crate) fn resolved(identity: &OwnershipIdentity) -> Self {
        Self::Resolved(identity.clone())
    }
    pub(crate) fn not_found(identity: &OwnershipIdentity) -> Self {
        Self::NotFound(identity.clone())
    }
    pub(crate) fn ambiguous(identities: &[OwnershipIdentity]) -> Self {
        Self::Ambiguous(identities.to_vec())
    }
    pub(crate) fn invalid_kind(identity: &OwnershipIdentity) -> Self {
        Self::InvalidKind(identity.clone())
    }
    pub(crate) fn argument_type_not_found(
        identity: &OwnershipIdentity,
        names: &[crate::sem::FullName],
    ) -> Self {
        Self::ArgumentTypeNotFound(OwnershipArgumentFailure {
            identity: identity.clone(),
            names: names.to_vec(),
        })
    }
    /// Type references whose lookup failed; NONE operator operands are not failures.
    pub fn unresolved_argument_types(&self) -> &[crate::sem::FullName] {
        if let Self::ArgumentTypeNotFound(failure) = self {
            &failure.names
        } else {
            &[]
        }
    }
    pub(crate) fn diagnostic_message(&self) -> String {
        match self {
            Self::Resolved(identity) => format!("resolved ownership target: {}", identity.name()),
            Self::NotFound(identity) => {
                format!("ownership target does not exist: {}", identity.name())
            }
            Self::InvalidKind(identity) => format!(
                "ownership target is not an object of the requested kind: {}",
                identity.name()
            ),
            Self::ArgumentTypeNotFound(failure) => format!(
                "argument type not found for ownership target: {}",
                failure.identity.name()
            ),
            Self::Ambiguous(_) => {
                "ownership target is ambiguous; specify an argument signature".to_owned()
            }
        }
    }
    /// Bound identity, available only for successful resolution.
    pub const fn identity(&self) -> Option<&OwnershipIdentity> {
        if let Self::Resolved(identity) = self {
            Some(identity)
        } else {
            None
        }
    }
    /// Stable outcome classification, independent of diagnostic wording.
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Resolved(_) => "resolved",
            Self::NotFound(_) => "ownership_target_not_found",
            Self::Ambiguous(_) => "ownership_target_ambiguous",
            Self::InvalidKind(_) => "ownership_invalid_target_kind",
            Self::ArgumentTypeNotFound(_) => "ownership_argument_type_not_found",
        }
    }
}

/// Analyzed ownership change, including completed target and owner lookup outcomes.
#[derive(Clone, Debug)]
pub struct AlterOwner {
    target: OwnershipLookup,
    owner: OwnerSpecification,
}
impl AlterOwner {
    pub(crate) fn new(target: &OwnershipLookup, owner: &OwnerSpecification) -> Self {
        Self {
            target: target.clone(),
            owner: owner.clone(),
        }
    }
    /// Completed target lookup (also retained when unsuccessful).
    pub const fn target(&self) -> &OwnershipLookup {
        &self.target
    }
    /// Explicit owner and its binding, or symbolic session specification.
    pub const fn owner(&self) -> &OwnerSpecification {
        &self.owner
    }
    pub(crate) const fn applied_identity(&self) -> Option<&OwnershipIdentity> {
        if self.owner.can_apply() {
            self.target.identity()
        } else {
            None
        }
    }
    /// Analyze and apply a PostgreSQL ALTER OWNER through the common AST.
    pub fn analyze_alter_owner<C: ParseContext>(
        context: C,
        syntax: crate::syn::AlterOwnerStmt,
    ) -> Result<AstAndContextPair<C>, AnalysisError> {
        crate::ast::AlterOwner::try_from_with_tokens(&syntax, &::sql_fun_core::IVec::default())?
            .resolve_and_apply(context)
    }
}

/// Failed argument-type lookup, retaining both the target and missing type names.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OwnershipArgumentFailure {
    identity: OwnershipIdentity,
    names: Vec<crate::sem::FullName>,
}
impl OwnershipArgumentFailure {
    /// Requested object identity with successful type bindings retained.
    pub const fn identity(&self) -> &OwnershipIdentity {
        &self.identity
    }
    /// Every missing argument type name in source order.
    pub fn names(&self) -> &[crate::sem::FullName] {
        &self.names
    }
}
