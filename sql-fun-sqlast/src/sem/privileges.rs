//! Retained PostgreSQL privilege declarations on catalog definitions.

use ::sql_fun_core::analysis_problem::PrivilegeProblem;

use crate::{
    ast::{
        DropBehavior, ObjectKind, PrivilegeGrantee,
        privileges::{
            ObjectPrivileges, PrivilegeAction, PrivilegeTarget, PrivilegeTargetKind,
            PrivilegeTargets, RoleMembership,
        },
    },
    context::ParseContext,
    names::{ColumnName, FullName},
    sem::{
        AnalysisError, AnalysisProblem, OwnerSpecification, OwnershipIdentity,
        ownership::RequestedIdentity, search_path::expand_relation_search_path,
    },
};

/// A grantee or grantor retained after role lookup.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum PrivilegeRole {
    /// The PostgreSQL pseudo-role PUBLIC.
    Public,
    /// A resolved named role or a symbolic session role.
    Role(OwnerSpecification),
}

/// The operation declared by a privilege statement.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum PrivilegeOperation {
    /// GRANT and its grant-option flag.
    Grant(bool),
    /// REVOKE, its grant-option-only flag and CASCADE flag.
    Revoke(bool, bool),
}

/// A privilege name and its explicitly listed columns.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct DeclaredPrivilege {
    name: String,
    columns: Vec<String>,
}

impl DeclaredPrivilege {
    /// The retained name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The retained columns.
    #[must_use]
    pub fn columns(&self) -> &[String] {
        &self.columns
    }
}

/// One object privilege declaration in source order.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ObjectPrivilegeDeclaration {
    operation: PrivilegeOperation,
    privileges: Vec<DeclaredPrivilege>,
    grantees: Vec<PrivilegeRole>,
    grantor: Option<PrivilegeRole>,
}

impl ObjectPrivilegeDeclaration {
    /// The retained operation.
    #[must_use]
    pub const fn operation(&self) -> &PrivilegeOperation {
        &self.operation
    }

    /// The retained privileges.
    #[must_use]
    pub fn privileges(&self) -> &[DeclaredPrivilege] {
        &self.privileges
    }

    /// The retained grantees.
    #[must_use]
    pub fn grantees(&self) -> &[PrivilegeRole] {
        &self.grantees
    }

    /// The retained grantor.
    #[must_use]
    pub const fn grantor(&self) -> Option<&PrivilegeRole> {
        self.grantor.as_ref()
    }
}

/// A validated object privilege declaration with its resolved targets.
#[derive(Clone, Debug)]
pub struct ObjectPrivilegeChange {
    targets: Vec<OwnershipIdentity>,
    schemas: Option<Vec<String>>,
    declaration: ObjectPrivilegeDeclaration,
    problems: Vec<PrivilegeProblem>,
}

impl ObjectPrivilegeChange {
    /// Failures recorded independently of diagnostic delivery.
    #[must_use]
    pub fn problems(&self) -> &[PrivilegeProblem] {
        &self.problems
    }

    /// Whether the whole declaration may update the catalog.
    #[must_use]
    pub const fn is_applicable(&self) -> bool {
        self.problems.is_empty()
    }

    /// Schema names in an ALL IN SCHEMA selector.
    #[must_use]
    pub fn schemas(&self) -> Option<&[String]> {
        self.schemas.as_deref()
    }

    /// The retained targets.
    #[must_use]
    pub fn targets(&self) -> &[OwnershipIdentity] {
        &self.targets
    }

    /// The retained declaration.
    #[must_use]
    pub const fn declaration(&self) -> &ObjectPrivilegeDeclaration {
        &self.declaration
    }
}

impl From<&PrivilegeAction> for PrivilegeOperation {
    fn from(action: &PrivilegeAction) -> Self {
        if let Some(behavior) = action.revoke_behavior() {
            Self::Revoke(
                action.revokes_grant_option_only(),
                behavior == DropBehavior::Cascade,
            )
        } else {
            Self::Grant(action.grants_with_grant_option())
        }
    }
}

fn postgres_major(context: &impl ParseContext) -> Option<u32> {
    context
        .get_args()
        .postgres_version(context.metadata())
        .ok()?
        .to_string()
        .parse::<::sql_fun_core::PostgresVersion>()
        .ok()
        .map(|version| version.major())
}

fn report(
    context: &mut impl ParseContext,
    problems: &[PrivilegeProblem],
) -> Result<(), AnalysisError> {
    for problem in problems {
        context.report_analysis_problem(AnalysisProblem::Privilege(problem.clone()))?;
    }
    Ok(())
}

impl PrivilegeRole {
    fn resolve(
        ast: &PrivilegeGrantee,
        public_allowed: bool,
        context: &impl ParseContext,
        problems: &mut Vec<PrivilegeProblem>,
    ) -> Self {
        let specification = match ast.keyword() {
            Some("CURRENT_ROLE") => OwnerSpecification::current_role(),
            Some("CURRENT_USER") => OwnerSpecification::current_user(),
            Some("SESSION_USER") => OwnerSpecification::session_user(),
            _ => {
                let Some(name) = ast.name() else {
                    if !public_allowed {
                        problems.push(PrivilegeProblem::PublicNotAllowed);
                    }
                    return Self::Public;
                };
                if let Some(role) = context.role_by_name(name) {
                    OwnerSpecification::role(role)
                } else {
                    problems.push(PrivilegeProblem::MissingRole(name.to_owned()));
                    OwnerSpecification::named(name)
                }
            }
        };
        Self::Role(specification)
    }
}

impl PrivilegeTargetKind {
    const fn object_kind(self) -> Option<ObjectKind> {
        Some(match self {
            Self::Table => ObjectKind::TABLE,
            Self::Sequence => ObjectKind::SEQUENCE,
            Self::Database => ObjectKind::DATABASE,
            Self::Domain => ObjectKind::DOMAIN,
            Self::ForeignDataWrapper => ObjectKind::FOREIGN_DATA_WRAPPER,
            Self::ForeignServer => ObjectKind::FOREIGN_SERVER,
            Self::Function => ObjectKind::FUNCTION,
            Self::Procedure => ObjectKind::PROCEDURE,
            Self::Routine => ObjectKind::ROUTINE,
            Self::Language => ObjectKind::LANGUAGE,
            Self::Schema => ObjectKind::SCHEMA,
            Self::Tablespace => ObjectKind::TABLESPACE,
            Self::Type => ObjectKind::TYPE,
            Self::LargeObject | Self::Parameter => return None,
        })
    }

    fn permits(self, name: &str, has_columns: bool, version: Option<u32>) -> bool {
        if has_columns {
            return self == Self::Table
                && matches!(name, "" | "select" | "insert" | "update" | "references");
        }
        if name.is_empty() {
            return true;
        }
        match self {
            Self::Table => {
                matches!(
                    name,
                    "select"
                        | "insert"
                        | "update"
                        | "delete"
                        | "truncate"
                        | "references"
                        | "trigger"
                ) || (name == "maintain" && version.is_none_or(|version| version >= 17))
            }
            Self::Sequence => matches!(name, "usage" | "select" | "update"),
            Self::Database => matches!(name, "create" | "connect" | "temporary" | "temp"),
            Self::Function | Self::Procedure | Self::Routine => name == "execute",
            Self::LargeObject => matches!(name, "select" | "update"),
            Self::Parameter => matches!(name, "set" | "alter system"),
            Self::Schema => matches!(name, "usage" | "create"),
            Self::Tablespace => name == "create",
            _ => name == "usage",
        }
    }
}

impl ObjectPrivilegeChange {
    pub(crate) fn analyze(
        ast: &ObjectPrivileges,
        context: &mut impl ParseContext,
    ) -> Result<Self, AnalysisError> {
        let mut problems = Vec::new();
        let targets = if let Some(kind) = ast.kind().object_kind() {
            Self::resolve_targets(ast.targets(), kind, context, &mut problems)?
        } else {
            let targets: Vec<_> = match ast.targets() {
                PrivilegeTargets::Objects(targets) => targets
                    .iter()
                    .filter_map(|target| match target {
                        PrivilegeTarget::LargeObjectId(id) => Some(id.clone()),
                        PrivilegeTarget::Name(name) => Some(name.parts().join(".")),
                        PrivilegeTarget::Routine(_) => None,
                    })
                    .collect(),
                PrivilegeTargets::AllInSchemas(schemas) => schemas.clone(),
            };
            problems.push(PrivilegeProblem::unsupported_targets(
                &format!("{:?}", ast.kind()),
                &targets,
            ));
            Vec::new()
        };
        let grantees = ast
            .grantees()
            .iter()
            .map(|role| {
                PrivilegeRole::resolve(
                    role,
                    !ast.action().grants_with_grant_option(),
                    context,
                    &mut problems,
                )
            })
            .collect();
        let grantor = ast
            .grantor()
            .map(|role| PrivilegeRole::resolve(role, false, context, &mut problems));
        let privileges: Vec<_> = ast
            .privileges()
            .iter()
            .map(|privilege| DeclaredPrivilege {
                name: privilege.name().to_owned(),
                columns: privilege.columns().to_vec(),
            })
            .collect();
        Self::validate_privileges(ast.kind(), &privileges, &targets, context, &mut problems);
        report(context, &problems)?;
        Ok(Self {
            targets,
            schemas: match ast.targets() {
                PrivilegeTargets::AllInSchemas(schemas) => Some(schemas.clone()),
                PrivilegeTargets::Objects(_) => None,
            },
            declaration: ObjectPrivilegeDeclaration {
                operation: ast.action().into(),
                privileges,
                grantees,
                grantor,
            },
            problems,
        })
    }

    fn validate_privileges(
        kind: PrivilegeTargetKind,
        privileges: &[DeclaredPrivilege],
        targets: &[OwnershipIdentity],
        context: &impl ParseContext,
        problems: &mut Vec<PrivilegeProblem>,
    ) {
        let version = postgres_major(context);
        for privilege in privileges {
            let sequence_usage = kind == PrivilegeTargetKind::Table
                && privilege.name() == "usage"
                && privilege.columns().is_empty()
                && !targets.is_empty()
                && targets
                    .iter()
                    .all(|target| target.kind() == ObjectKind::SEQUENCE);
            if !sequence_usage
                && !kind.permits(privilege.name(), !privilege.columns().is_empty(), version)
            {
                problems.push(PrivilegeProblem::invalid_privilege(
                    privilege.name(),
                    &format!("{kind:?}"),
                ));
            }
        }
        for identity in targets {
            if matches!(identity.kind(), ObjectKind::TYPE | ObjectKind::DOMAIN)
                && context
                    .get_type_impl(identity.name())
                    .is_some_and(|definition| definition.is_array() == Some(true))
            {
                problems.push(PrivilegeProblem::ArrayTarget(identity.into()));
            }
            for privilege in privileges {
                for column in &privilege.columns {
                    let name = identity.name();
                    let column_name = ColumnName::from(column.as_str());
                    if identity.kind() == ObjectKind::SEQUENCE
                        && matches!(column.as_str(), "last_value" | "log_cnt" | "is_called")
                    {
                        continue;
                    }
                    if !context
                        .get_table_impl(name)
                        .is_some_and(|table| table.has_column(&column_name))
                        && !context
                            .get_view_impl(name)
                            .is_some_and(|view| view.has_column(&column_name))
                    {
                        problems.push(PrivilegeProblem::missing_column(
                            &identity.into(),
                            &(&column_name).into(),
                        ));
                    }
                }
            }
        }
    }

    fn relation_target(name: &FullName, context: &impl ParseContext) -> Option<OwnershipIdentity> {
        expand_relation_search_path(context, name, |key| {
            let kind = if context.get_table_impl(key).is_some() {
                ObjectKind::TABLE
            } else if let Some(view) = context.get_view_impl(key) {
                if view.is_materialized() {
                    ObjectKind::MATERIALIZED_VIEW
                } else {
                    ObjectKind::VIEW
                }
            } else if context.get_sequence_impl(key).is_some() {
                ObjectKind::SEQUENCE
            } else {
                return None;
            };
            Some(OwnershipIdentity::named(kind, key))
        })
    }

    fn resolve_targets(
        ast: &PrivilegeTargets,
        kind: ObjectKind,
        context: &mut impl ParseContext,
        problems: &mut Vec<PrivilegeProblem>,
    ) -> Result<Vec<OwnershipIdentity>, AnalysisError> {
        let mut identities = Vec::new();
        match ast {
            PrivilegeTargets::Objects(objects) => {
                for target in objects {
                    if let PrivilegeTarget::Name(name) = target
                        && kind == ObjectKind::TABLE
                    {
                        let name = FullName::from_ast_name(name)?;
                        let Some(identity) = Self::relation_target(&name, context) else {
                            problems.push(PrivilegeProblem::MissingRelation((&name).into()));
                            continue;
                        };
                        if !identities.contains(&identity) {
                            identities.push(identity);
                        }
                        continue;
                    }
                    let requested = match target {
                        PrivilegeTarget::Name(name) => RequestedIdentity::new(
                            &OwnershipIdentity::named(kind, &FullName::from_ast_name(name)?),
                            &[],
                        ),
                        PrivilegeTarget::Routine(routine) => {
                            RequestedIdentity::from_ast_object_target(routine, kind, context)?
                        }
                        PrivilegeTarget::LargeObjectId(_) => {
                            unreachable!("large objects have no catalog kind")
                        }
                    };
                    let outcome = requested.resolve(context);
                    let Some(identity) = outcome.identity() else {
                        problems.extend(outcome.privilege_problem());
                        continue;
                    };
                    if !identities.contains(identity) {
                        identities.push(identity.clone());
                    }
                }
            }
            PrivilegeTargets::AllInSchemas(schemas) => {
                for schema in schemas {
                    let name = FullName::from_local_name(schema);
                    if RequestedIdentity::new(
                        &OwnershipIdentity::named(ObjectKind::SCHEMA, &name),
                        &[],
                    )
                    .resolve(context)
                    .identity()
                    .is_none()
                    {
                        problems.push(PrivilegeProblem::MissingSchema(schema.clone()));
                        continue;
                    }
                    for name in match kind {
                        ObjectKind::TABLE => context.relation_names(),
                        ObjectKind::SEQUENCE => context.sequence_names(),
                        _ => context.routine_names(),
                    } {
                        if name
                            .schema()
                            .is_some_and(|name| name.canonical().as_str() == schema.as_str())
                        {
                            if kind == ObjectKind::TABLE {
                                if let Some(identity) = Self::relation_target(&name, context)
                                    && identity.kind() != ObjectKind::SEQUENCE
                                    && !identities.contains(&identity)
                                {
                                    identities.push(identity);
                                }
                                continue;
                            }
                            for definition in context.ownership_definitions(kind, &name) {
                                if !identities.contains(definition.identity()) {
                                    identities.push(definition.identity().clone());
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(identities)
    }
}

/// Validated role membership declaration, retained on each granted role.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct RoleMembershipDeclaration {
    operation: PrivilegeOperation,
    roles: Vec<String>,
    members: Vec<PrivilegeRole>,
    grantor: Option<PrivilegeRole>,
    options: Vec<(String, bool)>,
    problems: Vec<PrivilegeProblem>,
}

impl RoleMembershipDeclaration {
    /// Failures recorded independently of diagnostic delivery.
    #[must_use]
    pub fn problems(&self) -> &[PrivilegeProblem] {
        &self.problems
    }

    /// Whether the whole declaration may update the catalog.
    #[must_use]
    pub const fn is_applicable(&self) -> bool {
        self.problems.is_empty()
    }

    /// The retained operation.
    #[must_use]
    pub const fn operation(&self) -> &PrivilegeOperation {
        &self.operation
    }

    /// The retained roles.
    #[must_use]
    pub fn roles(&self) -> &[String] {
        &self.roles
    }

    /// The retained members.
    #[must_use]
    pub fn members(&self) -> &[PrivilegeRole] {
        &self.members
    }

    /// The retained grantor.
    #[must_use]
    pub const fn grantor(&self) -> Option<&PrivilegeRole> {
        self.grantor.as_ref()
    }

    /// The retained options.
    #[must_use]
    pub fn options(&self) -> &[(String, bool)] {
        &self.options
    }
}

impl RoleMembershipDeclaration {
    pub(crate) fn analyze(
        ast: &RoleMembership,
        context: &mut impl ParseContext,
    ) -> Result<Self, AnalysisError> {
        let mut problems = Vec::new();
        let mut roles = Vec::new();
        for role in ast.roles() {
            let name = role.name();
            if !role.columns().is_empty() {
                problems.push(PrivilegeProblem::GrantedRoleColumns(name.to_owned()));
            }
            if context.role_by_name(name).is_none() {
                problems.push(PrivilegeProblem::MissingRole(name.to_owned()));
            }
            if !roles.iter().any(|role| role == name) {
                roles.push(name.to_owned());
            }
        }
        let members = ast
            .members()
            .iter()
            .map(|role| PrivilegeRole::resolve(role, false, context, &mut problems))
            .collect();
        let grantor = ast
            .grantor()
            .map(|role| PrivilegeRole::resolve(role, false, context, &mut problems));
        let version = postgres_major(context);
        let mut options = Vec::new();
        for option in ast.options() {
            let name = option.name();
            if !matches!(name, "admin" | "inherit" | "set")
                || (version.is_some_and(|version| version < 16)
                    && (name != "admin"
                        || option.uses_explicit_boolean()
                        || (ast.is_grant() && !option.value())))
                || options.iter().any(|(existing, _)| existing == name)
            {
                problems.push(PrivilegeProblem::MembershipOption(name.to_owned()));
            }
            options.push((name.to_owned(), option.value()));
        }
        report(context, &problems)?;
        Ok(Self {
            operation: ast.action().into(),
            roles,
            members,
            grantor,
            options,
            problems,
        })
    }
}
