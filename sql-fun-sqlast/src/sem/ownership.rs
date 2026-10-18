//! Semantic identities and ownership of PostgreSQL catalog objects.
use crate::sem::{FullName, ObjectKind, TypeReference};

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
        self.kind.accepts_owner_kind(definition.kind)
            && self.name == definition.name
            && self.access_method == definition.access_method
            && (self.arguments.is_none() || self.arguments == definition.arguments)
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

/// Exact-name role lookup supplied by the explicitly selected analysis environment.
pub trait RoleInfoRead {
    /// Missing roles remain missing; implementations must not fabricate a role.
    fn role_by_name(&self, _name: &str) -> Option<&Role> {
        None
    }
}

/// Exact catalog reads used for ownership analysis (no search-path expansion here).
pub trait OwnershipInfoRead {
    /// Retained schema DDL by exact global schema name.
    fn schema_definition(&self, _name: &str) -> Option<&crate::sem::CreateSchema> {
        None
    }

    /// Retained routine DDL, one definition per exact input signature.
    fn routine_definitions(&self, _name: &FullName) -> Vec<crate::sem::CreateFunction> {
        Vec::new()
    }

    /// Existing definitions of the requested class at this exact name.
    /// ROUTINE includes functions and procedures. Return one item per overload.
    fn ownership_definitions(
        &self,
        _kind: ObjectKind,
        _name: &FullName,
    ) -> Vec<OwnedObjectDefinition> {
        Vec::new()
    }
}

impl ObjectKind {
    pub(crate) fn accepts_owner_kind(self, actual: Self) -> bool {
        self == actual
            || (self.code() == 35 && matches!(actual.code(), 20 | 30))
            || (self.code() == 50 && actual.code() == 13)
    }
    pub(crate) const fn owner_schema_scoped(self) -> bool {
        matches!(
            self.code(),
            2 | 8 | 9 | 13 | 20 | 24 | 25 | 26 | 27 | 30 | 35 | 38 | 40 | 42 | 46 | 47 | 50 | 52
        )
    }
    pub(crate) const fn valid_owner_kind(self) -> bool {
        matches!(
            self.code(),
            2 | 8
                | 9
                | 10
                | 13
                | 15
                | 17
                | 18
                | 20
                | 22
                | 23
                | 24
                | 25
                | 26
                | 27
                | 30
                | 31
                | 35
                | 37
                | 38
                | 39
                | 40
                | 42
                | 43
                | 46
                | 47
                | 50
                | 52
        )
    }
}

/// Layer-local inventory for object classes without richer DDL representations.
#[derive(Debug, Default)]
pub(crate) struct OwnershipCatalog {
    roles: std::collections::HashMap<String, Role>,
    definitions: Vec<OwnedObjectDefinition>,
}
impl OwnershipCatalog {
    pub(crate) fn insert_role(&mut self, role: &Role) {
        self.roles.insert(role.name().to_owned(), role.clone());
    }
    pub(crate) fn role(&self, name: &str) -> Option<&Role> {
        self.roles.get(name)
    }
    pub(crate) fn insert(&mut self, definition: &OwnedObjectDefinition) {
        self.definitions
            .retain(|existing| existing.identity() != definition.identity());
        self.definitions.push(definition.clone());
    }
    pub(crate) fn definitions(
        &self,
        kind: ObjectKind,
        name: &FullName,
    ) -> Vec<OwnedObjectDefinition> {
        self.definitions
            .iter()
            .filter(|definition| {
                kind.accepts_owner_kind(definition.identity.kind)
                    && definition.identity.name() == name
            })
            .cloned()
            .collect()
    }
}

pub(crate) fn model_definitions<C: crate::sem::BaseParseContext + ?Sized>(
    context: &C,
    kind: ObjectKind,
    name: &FullName,
) -> Vec<OwnedObjectDefinition> {
    let mut identity = OwnershipIdentity::named(kind, name);
    let owner = match kind.code() {
        42 => context
            .get_table_impl(name)
            .map(|definition| definition.owner()),
        38 => context
            .get_sequence_impl(name)
            .map(|definition| definition.owner()),
        24 | 52 => context
            .get_view_impl(name)
            .filter(|view| view.is_materialized() == (kind.code() == 24))
            .map(|view| view.owner()),
        37 => context
            .schema_definition(name.name().as_str())
            .map(|definition| definition.owner()),
        13 | 50 => context
            .get_type_impl(name)
            .filter(|definition| kind.code() == 50 || definition.is_domain())
            .map(|definition| {
                if definition.is_domain() {
                    identity = OwnershipIdentity::named(ObjectKind::from(13), name);
                }
                if definition.is_relation_type() {
                    if let Some(table) = context.get_table_impl(name) {
                        table.owner()
                    } else {
                        None
                    }
                } else {
                    definition.owner()
                }
            }),
        20 | 30 | 35 => {
            return context
                .routine_definitions(name)
                .into_iter()
                .filter(|definition| kind.accepts_owner_kind(definition.object_kind()))
                .map(|definition| {
                    OwnedObjectDefinition::new(
                        &definition.ownership_identity(name),
                        definition.owner(),
                    )
                })
                .collect();
        }
        _ => None,
    };
    owner.map_or_else(Vec::new, |owner| {
        vec![OwnedObjectDefinition::new(&identity, owner)]
    })
}
