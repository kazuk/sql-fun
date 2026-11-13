mod ddl;
use crate::{
    ast::ObjectKind,
    names::FullName as NamesFullName,
    sem::{
        AnalysisError, OwnerSpecification, OwnershipIdentity, TypeReference,
        traversal::SemanticVisitor,
    },
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
        names: &[NamesFullName],
    ) -> Self {
        Self::ArgumentTypeNotFound(OwnershipArgumentFailure {
            identity: identity.clone(),
            names: names.to_vec(),
        })
    }
    /// Type references whose lookup failed; NONE operator operands are not failures.
    pub fn unresolved_argument_types(&self) -> &[NamesFullName] {
        if let Self::ArgumentTypeNotFound(failure) = self {
            &failure.names
        } else {
            &[]
        }
    }
    pub(crate) fn diagnostic_message(&self) -> String {
        match self {
            Self::Resolved(identity) => format!("resolved ownership target: {identity}"),
            Self::NotFound(identity) => {
                format!("ownership target does not exist: {identity}")
            }
            Self::InvalidKind(identity) => {
                format!("ownership target is not an object of the requested kind: {identity}")
            }
            Self::ArgumentTypeNotFound(failure) => {
                let names = failure
                    .names
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "argument type not found for ownership target {}: {names}",
                    failure.identity.name()
                )
            }
            Self::Ambiguous(identities) => {
                let candidates = identities
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "ownership target is ambiguous; specify an argument signature; candidates: {candidates}"
                )
            }
        }
    }
}

#[cfg(test)]
mod lookup_message_tests {
    use crate::{
        ast::ObjectKind,
        names::FullName,
        sem::{OwnershipIdentity, OwnershipLookup, PgBuiltInType},
    };

    #[test]
    fn failed_lookup_keeps_the_requested_signature() {
        let name: FullName = "public.compute".parse().unwrap();
        let identity = OwnershipIdentity::with_arguments(
            ObjectKind::FUNCTION,
            &name,
            &[Some(PgBuiltInType::int4())],
        );
        assert_eq!(
            OwnershipLookup::not_found(&identity).diagnostic_message(),
            "ownership target does not exist: function public.compute(pg_catalog.int4)"
        );
    }

    #[test]
    fn missing_type_names_are_reported_without_inventing_operands() {
        let name: FullName = "public.compute".parse().unwrap();
        let missing: FullName = r#"public."missing.type""#.parse().unwrap();
        let identity = OwnershipIdentity::with_arguments(ObjectKind::FUNCTION, &name, &[None]);
        let problem = OwnershipLookup::argument_type_not_found(&identity, &[missing]);
        assert_eq!(
            problem.diagnostic_message(),
            r#"argument type not found for ownership target public.compute: public."missing.type""#
        );
    }
}

impl OwnershipLookup {
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
    pub(crate) fn visit_references<V: SemanticVisitor>(
        &self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        let Some(identity) = self.target.identity() else {
            return Ok(());
        };
        match identity.kind() {
            ObjectKind::TABLE => visitor.schema_relation(identity.name(), false)?,
            ObjectKind::VIEW | ObjectKind::MATERIALIZED_VIEW => {
                visitor.schema_relation(identity.name(), true)?
            }
            ObjectKind::FUNCTION => visitor.schema_function(identity.name())?,
            ObjectKind::TYPE | ObjectKind::DOMAIN => {
                visitor.schema_type(&TypeReference::from_full_name(identity.name().clone()))?
            }
            _ => {}
        }
        if let Some(arguments) = identity.arguments() {
            for reference in arguments.iter().flatten() {
                visitor.schema_type(reference)?;
            }
        }
        Ok(())
    }

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
}

#[cfg(test)]
mod test_missing_owner_does_not_replace_previously_committed_ownership {
    use crate::{
        context::OwnershipInfoRead,
        sem::Role,
        test_helpers::ownership::{apply_ownership_sql, ownership_context},
    };

    #[test]
    fn missing_owner_does_not_replace_previously_committed_ownership() -> testresult::TestResult {
        let context = apply_ownership_sql(ownership_context(), "CREATE SCHEMA app")?.into_context();
        let context =
            apply_ownership_sql(context, "ALTER SCHEMA app OWNER TO alice")?.into_context();
        let result = apply_ownership_sql(context, "ALTER SCHEMA app OWNER TO missing")?;
        assert_eq!(
            result
                .context()
                .schema_definition("app")
                .unwrap()
                .owner()
                .unwrap()
                .resolved_role()
                .map(Role::name),
            Some("alice")
        );
        let result = apply_ownership_sql(result.into_context(), "ALTER SCHEMA app OWNER TO bob")?;
        assert_eq!(
            result
                .context()
                .schema_definition("app")
                .unwrap()
                .owner()
                .unwrap()
                .resolved_role()
                .map(Role::name),
            Some("bob")
        );
        Ok(())
    }
}

/// Failed argument-type lookup, retaining both the target and missing type names.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OwnershipArgumentFailure {
    identity: OwnershipIdentity,
    names: Vec<NamesFullName>,
}
impl OwnershipArgumentFailure {
    /// Requested object identity with successful type bindings retained.
    pub const fn identity(&self) -> &OwnershipIdentity {
        &self.identity
    }
    /// Every missing argument type name in source order.
    pub fn names(&self) -> &[NamesFullName] {
        &self.names
    }
}
