//! Semantic identities and ownership of PostgreSQL catalog objects.

mod ddl;
use crate::{
    ast::ObjectKind,
    context::BaseParseContext,
    names::{FullName, OperatorName, SchemaName},
    sem::TypeReference,
};

/// A role supplied by the analysis environment, independent of SQL spelling.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct Role(String);
impl From<&str> for Role {
    fn from(name: &str) -> Self {
        Self(name.to_owned())
    }
}
impl Role {
    /// Exact role name (SQL quoting has already been decoded).
    pub fn name(&self) -> &str {
        &self.0
    }
}

/// Explicit owner syntax and, for named roles, its completed lookup result.
/// Session keywords remain symbolic; no connection or ambient identity is inferred.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct OwnerSpecification {
    kind: OwnerKind,
    role: Option<Role>,
}
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
enum OwnerKind {
    Named(String),
    CurrentRole,
    CurrentUser,
    SessionUser,
}
impl OwnerKind {
    fn named(name: &str) -> Self {
        Self::Named(name.to_owned())
    }
    const fn current_role() -> Self {
        Self::CurrentRole
    }
    const fn current_user() -> Self {
        Self::CurrentUser
    }
    const fn session_user() -> Self {
        Self::SessionUser
    }
    const fn name(&self) -> Option<&str> {
        if let Self::Named(name) = self {
            Some(name.as_str())
        } else {
            None
        }
    }
    const fn session_keyword(&self) -> Option<&'static str> {
        match self {
            Self::CurrentRole => Some("CURRENT_ROLE"),
            Self::CurrentUser => Some("CURRENT_USER"),
            Self::SessionUser => Some("SESSION_USER"),
            Self::Named(_) => None,
        }
    }
}

impl OwnerSpecification {
    /// An explicit named owner before lookup (or a missing named role afterwards).
    pub fn named(name: &str) -> Self {
        Self {
            kind: OwnerKind::named(name),
            role: None,
        }
    }
    /// A successfully bound named owner.
    pub fn role(role: &Role) -> Self {
        Self {
            kind: OwnerKind::named(role.name()),
            role: Some(role.clone()),
        }
    }
    /// CURRENT_ROLE, without assuming a concrete session identity.
    pub const fn current_role() -> Self {
        Self {
            kind: OwnerKind::current_role(),
            role: None,
        }
    }
    /// CURRENT_USER, without assuming a concrete session identity.
    pub const fn current_user() -> Self {
        Self {
            kind: OwnerKind::current_user(),
            role: None,
        }
    }
    /// SESSION_USER, without assuming a concrete session identity.
    pub const fn session_user() -> Self {
        Self {
            kind: OwnerKind::session_user(),
            role: None,
        }
    }
    /// Exact explicit role name; absent for session-dependent specifications.
    pub const fn name(&self) -> Option<&str> {
        self.kind.name()
    }
    /// Bound role; absent for missing named roles and symbolic session owners.
    pub const fn resolved_role(&self) -> Option<&Role> {
        self.role.as_ref()
    }
    /// PostgreSQL session keyword, absent for named roles.
    pub const fn session_keyword(&self) -> Option<&'static str> {
        self.kind.session_keyword()
    }
    /// Whether this is a symbolic session specification.
    pub const fn is_session_dependent(&self) -> bool {
        self.kind.session_keyword().is_some()
    }
    pub(crate) const fn can_apply(&self) -> bool {
        self.is_session_dependent() || self.role.is_some()
    }
}

/// Exact catalog identity, including overload signature and access method.
/// An absent signature differs from an explicitly empty signature.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct OwnershipIdentity {
    kind: ObjectKind,
    name: FullName,
    arguments: Option<Vec<Option<TypeReference>>>,
    access_method: Option<String>,
}
impl OwnershipIdentity {
    /// Identity of an object which does not have an argument signature.
    pub fn named(kind: ObjectKind, name: &FullName) -> Self {
        Self {
            kind,
            name: name.clone(),
            arguments: None,
            access_method: None,
        }
    }
    /// Routine, aggregate, or operator identity. NONE operator operands are absent types.
    pub fn with_arguments(
        kind: ObjectKind,
        name: &FullName,
        arguments: &[Option<TypeReference>],
    ) -> Self {
        Self {
            kind,
            name: name.clone(),
            arguments: Some(arguments.to_vec()),
            access_method: None,
        }
    }
    /// Operator class or family identity within an access method.
    pub fn with_access_method(kind: ObjectKind, name: &FullName, method: &str) -> Self {
        Self {
            kind,
            name: name.clone(),
            arguments: None,
            access_method: Some(method.to_owned()),
        }
    }
    /// Actual PostgreSQL object kind, not the ROUTINE lookup category.
    pub const fn kind(&self) -> ObjectKind {
        self.kind
    }
    /// Qualified catalog key for schema-scoped definitions; exact name for global objects.
    pub const fn name(&self) -> &FullName {
        &self.name
    }
    /// Exact input type signature, distinguishing omission and ().
    pub fn arguments(&self) -> Option<&[Option<TypeReference>]> {
        self.arguments.as_deref()
    }
    /// Access method for an operator class or family.
    pub fn access_method(&self) -> Option<&str> {
        self.access_method.as_deref()
    }
    pub(crate) fn with_name(&self, name: &FullName) -> Self {
        let mut value = self.clone();
        value.name = name.clone();
        value
    }
    pub(crate) fn matches(&self, definition: &Self) -> bool {
        Self::accepts_owner_kind(self.kind, definition.kind)
            && self.name == definition.name
            && self.access_method == definition.access_method
            && (self.arguments.is_none() || self.arguments == definition.arguments)
    }
}

impl std::fmt::Display for OwnershipIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ", Self::owner_kind_label(self.kind))?;
        if self.kind == ObjectKind::OPERATOR {
            if let Some(catalog) = self.name.catalog() {
                write!(
                    f,
                    "{}.",
                    crate::identifier::CanonicalIdentifier::from(catalog.as_str())
                )?;
            }
            let name = OperatorName::from_parts(
                self.name.schema().map(SchemaName::as_str),
                self.name.name().as_str(),
            );
            write!(f, "{name}")?;
        } else {
            write!(f, "{}", self.name)?;
        }
        if let Some(arguments) = &self.arguments {
            f.write_str("(")?;
            for (index, argument) in arguments.iter().enumerate() {
                if index > 0 {
                    f.write_str(", ")?;
                }
                if let Some(ty) = argument {
                    write!(f, "{ty}")?;
                } else if self.kind == ObjectKind::OPERATOR {
                    f.write_str("NONE")?;
                } else {
                    f.write_str("unresolved type")?;
                }
            }
            f.write_str(")")?;
        }
        if let Some(method) = &self.access_method {
            write!(
                f,
                " USING {}",
                crate::identifier::CanonicalIdentifier::from(method.as_str())
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod identity_display_tests {
    use crate::{
        ast::ObjectKind,
        names::FullName,
        sem::{OwnershipIdentity, PgBuiltInType},
    };

    #[test]
    fn preserves_omitted_and_explicit_signatures() {
        let name: FullName = "public.compute".parse().unwrap();
        assert_eq!(
            OwnershipIdentity::named(ObjectKind::FUNCTION, &name).to_string(),
            "function public.compute"
        );
        assert_eq!(
            OwnershipIdentity::with_arguments(ObjectKind::FUNCTION, &name, &[]).to_string(),
            "function public.compute()"
        );
        assert_eq!(
            OwnershipIdentity::with_arguments(
                ObjectKind::FUNCTION,
                &name,
                &[Some(PgBuiltInType::int4())]
            )
            .to_string(),
            "function public.compute(pg_catalog.int4)"
        );
    }

    #[test]
    fn presents_operator_operands_and_access_method() {
        let name: FullName = r#"public."+""#.parse().unwrap();
        let identity = OwnershipIdentity::with_arguments(
            ObjectKind::OPERATOR,
            &name,
            &[None, Some(PgBuiltInType::int4())],
        );
        assert_eq!(
            identity.to_string(),
            "operator public.+(NONE, pg_catalog.int4)"
        );
        let name: FullName = r#"public."sort.values""#.parse().unwrap();
        let identity =
            OwnershipIdentity::with_access_method(ObjectKind::OPERATOR_CLASS, &name, "a\"b");
        assert_eq!(
            identity.to_string(),
            r#"operator class public."sort.values" USING "a""b""#
        );
    }
}

/// Minimal retained definition for catalog classes without a richer DDL model.
/// Registration establishes existence; ALTER OWNER never registers an absent target.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OwnedObjectDefinition {
    identity: OwnershipIdentity,
    owner: Option<OwnerSpecification>,
}
impl OwnedObjectDefinition {
    pub(crate) fn contains_identity(definitions: &[Self], identity: &OwnershipIdentity) -> bool {
        for definition in definitions {
            if definition.identity() == identity {
                return true;
            }
        }
        false
    }

    /// Retain a known existing definition and its known (or unknown) ownership.
    pub fn new(identity: &OwnershipIdentity, owner: Option<&OwnerSpecification>) -> Self {
        Self {
            identity: identity.clone(),
            owner: owner.cloned(),
        }
    }
    /// Exact definition identity.
    pub const fn identity(&self) -> &OwnershipIdentity {
        &self.identity
    }
    /// Retained owner; None means ownership was not supplied.
    pub const fn owner(&self) -> Option<&OwnerSpecification> {
        self.owner.as_ref()
    }
    /// Replace ownership immediately, preserving the definition identity.
    pub fn change_owner(&mut self, owner: &OwnerSpecification) {
        self.owner = Some(owner.clone());
    }
}

impl OwnershipIdentity {
    const fn owner_kind_label(kind: ObjectKind) -> &'static str {
        match kind {
            ObjectKind::AGGREGATE => "aggregate",
            ObjectKind::COLLATION => "collation",
            ObjectKind::CONVERSION => "conversion",
            ObjectKind::DATABASE => "database",
            ObjectKind::DOMAIN => "domain",
            ObjectKind::EVENT_TRIGGER => "event trigger",
            ObjectKind::FOREIGN_DATA_WRAPPER => "foreign data wrapper",
            ObjectKind::FOREIGN_SERVER => "foreign server",
            ObjectKind::FUNCTION => "function",
            ObjectKind::LANGUAGE => "language",
            ObjectKind::LARGE_OBJECT => "large object",
            ObjectKind::MATERIALIZED_VIEW => "materialized view",
            ObjectKind::OPERATOR_CLASS => "operator class",
            ObjectKind::OPERATOR => "operator",
            ObjectKind::OPERATOR_FAMILY => "operator family",
            ObjectKind::PROCEDURE => "procedure",
            ObjectKind::PUBLICATION => "publication",
            ObjectKind::ROUTINE => "routine",
            ObjectKind::SCHEMA => "schema",
            ObjectKind::SEQUENCE => "sequence",
            ObjectKind::SUBSCRIPTION => "subscription",
            ObjectKind::STATISTICS => "statistics",
            ObjectKind::TABLE => "table",
            ObjectKind::TABLESPACE => "tablespace",
            ObjectKind::TEXT_SEARCH_CONFIGURATION => "text search configuration",
            ObjectKind::TEXT_SEARCH_DICTIONARY => "text search dictionary",
            ObjectKind::TYPE => "type",
            ObjectKind::VIEW => "view",
            _ => "object",
        }
    }
    pub(crate) fn accepts_owner_kind(kind: ObjectKind, actual: ObjectKind) -> bool {
        kind == actual
            || (kind == ObjectKind::ROUTINE
                && matches!(actual, ObjectKind::FUNCTION | ObjectKind::PROCEDURE))
            || (kind == ObjectKind::TYPE && actual == ObjectKind::DOMAIN)
    }
    pub(crate) const fn owner_schema_scoped(kind: ObjectKind) -> bool {
        matches!(
            kind,
            ObjectKind::AGGREGATE
                | ObjectKind::COLLATION
                | ObjectKind::CONVERSION
                | ObjectKind::DOMAIN
                | ObjectKind::FUNCTION
                | ObjectKind::MATERIALIZED_VIEW
                | ObjectKind::OPERATOR_CLASS
                | ObjectKind::OPERATOR
                | ObjectKind::OPERATOR_FAMILY
                | ObjectKind::PROCEDURE
                | ObjectKind::ROUTINE
                | ObjectKind::SEQUENCE
                | ObjectKind::STATISTICS
                | ObjectKind::TABLE
                | ObjectKind::TEXT_SEARCH_CONFIGURATION
                | ObjectKind::TEXT_SEARCH_DICTIONARY
                | ObjectKind::TYPE
                | ObjectKind::VIEW
        )
    }
    pub(crate) const fn valid_owner_kind(kind: ObjectKind) -> bool {
        matches!(
            kind,
            ObjectKind::AGGREGATE
                | ObjectKind::COLLATION
                | ObjectKind::CONVERSION
                | ObjectKind::DATABASE
                | ObjectKind::DOMAIN
                | ObjectKind::EVENT_TRIGGER
                | ObjectKind::FOREIGN_DATA_WRAPPER
                | ObjectKind::FOREIGN_SERVER
                | ObjectKind::FUNCTION
                | ObjectKind::LANGUAGE
                | ObjectKind::LARGE_OBJECT
                | ObjectKind::MATERIALIZED_VIEW
                | ObjectKind::OPERATOR_CLASS
                | ObjectKind::OPERATOR
                | ObjectKind::OPERATOR_FAMILY
                | ObjectKind::PROCEDURE
                | ObjectKind::PUBLICATION
                | ObjectKind::ROUTINE
                | ObjectKind::SCHEMA
                | ObjectKind::SEQUENCE
                | ObjectKind::SUBSCRIPTION
                | ObjectKind::STATISTICS
                | ObjectKind::TABLE
                | ObjectKind::TABLESPACE
                | ObjectKind::TEXT_SEARCH_CONFIGURATION
                | ObjectKind::TEXT_SEARCH_DICTIONARY
                | ObjectKind::TYPE
                | ObjectKind::VIEW
        )
    }
}

pub(crate) fn model_definitions<C: BaseParseContext + ?Sized>(
    context: &C,
    kind: ObjectKind,
    name: &FullName,
) -> Vec<OwnedObjectDefinition> {
    match kind {
        ObjectKind::TABLE => table_definitions(context, kind, name),
        ObjectKind::SEQUENCE => sequence_definitions(context, kind, name),
        ObjectKind::MATERIALIZED_VIEW | ObjectKind::VIEW => view_definitions(context, kind, name),
        ObjectKind::SCHEMA => schema_definitions(context, kind, name),
        ObjectKind::DOMAIN | ObjectKind::TYPE => type_definitions(context, kind, name),
        ObjectKind::FUNCTION | ObjectKind::PROCEDURE | ObjectKind::ROUTINE => {
            routine_definitions(context, kind, name)
        }
        _ => Vec::new(),
    }
}

fn table_definitions<C: BaseParseContext + ?Sized>(
    context: &C,
    kind: ObjectKind,
    name: &FullName,
) -> Vec<OwnedObjectDefinition> {
    let Some(definition) = context.get_table_impl(name) else {
        return Vec::new();
    };
    ownership_snapshot(kind, name, definition.owner())
}

fn sequence_definitions<C: BaseParseContext + ?Sized>(
    context: &C,
    kind: ObjectKind,
    name: &FullName,
) -> Vec<OwnedObjectDefinition> {
    let Some(definition) = context.get_sequence_impl(name) else {
        return Vec::new();
    };
    ownership_snapshot(kind, name, definition.owner())
}

fn view_definitions<C: BaseParseContext + ?Sized>(
    context: &C,
    kind: ObjectKind,
    name: &FullName,
) -> Vec<OwnedObjectDefinition> {
    let Some(definition) = context
        .get_view_impl(name)
        .filter(|view| view.is_materialized() == (kind == ObjectKind::MATERIALIZED_VIEW))
    else {
        return Vec::new();
    };
    ownership_snapshot(kind, name, definition.owner())
}

fn schema_definitions<C: BaseParseContext + ?Sized>(
    context: &C,
    kind: ObjectKind,
    name: &FullName,
) -> Vec<OwnedObjectDefinition> {
    let Some(definition) = context.schema_definition(name.name().as_str()) else {
        return Vec::new();
    };
    ownership_snapshot(kind, name, definition.owner())
}

fn type_definitions<C: BaseParseContext + ?Sized>(
    context: &C,
    kind: ObjectKind,
    name: &FullName,
) -> Vec<OwnedObjectDefinition> {
    let Some(definition) = context
        .get_type_impl(name)
        .filter(|definition| kind == ObjectKind::TYPE || definition.is_domain())
    else {
        return Vec::new();
    };
    let actual_kind = if definition.is_domain() {
        ObjectKind::DOMAIN
    } else {
        kind
    };
    let owner = if definition.is_relation_type() {
        if let Some(table) = context.get_table_impl(name) {
            table.owner()
        } else {
            None
        }
    } else {
        definition.owner()
    };
    ownership_snapshot(actual_kind, name, owner)
}

fn routine_definitions<C: BaseParseContext + ?Sized>(
    context: &C,
    kind: ObjectKind,
    name: &FullName,
) -> Vec<OwnedObjectDefinition> {
    context
        .routine_definitions(name)
        .into_iter()
        .filter(|definition| OwnershipIdentity::accepts_owner_kind(kind, definition.object_kind()))
        .map(|definition| {
            OwnedObjectDefinition::new(&definition.ownership_identity(name), definition.owner())
        })
        .collect()
}

fn ownership_snapshot(
    kind: ObjectKind,
    name: &FullName,
    owner: Option<&OwnerSpecification>,
) -> Vec<OwnedObjectDefinition> {
    vec![OwnedObjectDefinition::new(
        &OwnershipIdentity::named(kind, name),
        owner,
    )]
}
