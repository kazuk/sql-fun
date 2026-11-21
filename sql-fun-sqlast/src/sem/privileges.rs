//! Retained PostgreSQL privilege declarations on catalog definitions.

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
        AnalysisError, AnalysisProblem, OwnerSpecification, OwnershipIdentity, Role,
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
    /// PostgreSQL privilege name. An empty name with columns means ALL on those columns.
    pub name: String,
    /// Exact column names supplied by the declaration.
    pub columns: Vec<String>,
}

/// One object privilege declaration in source order.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ObjectPrivilegeDeclaration {
    /// GRANT or REVOKE and its options.
    pub operation: PrivilegeOperation,
    /// Privilege names and their column lists. An empty list means ALL.
    pub privileges: Vec<DeclaredPrivilege>,
    /// Grantees in declaration order.
    pub grantees: Vec<PrivilegeRole>,
    /// Explicit grantor, when supplied.
    pub grantor: Option<PrivilegeRole>,
}

/// A validated object privilege declaration with its resolved targets.
#[derive(Clone, Debug)]
pub struct ObjectPrivilegeChange {
    pub(crate) targets: Vec<OwnershipIdentity>,
    pub(crate) declaration: ObjectPrivilegeDeclaration,
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

fn report(context: &mut impl ParseContext, message: String) -> Result<(), AnalysisError> {
    context.report_analysis_problem(AnalysisProblem::Privilege(message))
}

impl PrivilegeRole {
    fn resolve(
        ast: &PrivilegeGrantee,
        public_allowed: bool,
        context: &mut impl ParseContext,
    ) -> Result<Option<Self>, AnalysisError> {
        let specification = match ast.keyword() {
            Some("PUBLIC") if public_allowed => return Ok(Some(Self::Public)),
            Some("CURRENT_ROLE") => OwnerSpecification::current_role(),
            Some("CURRENT_USER") => OwnerSpecification::current_user(),
            Some("SESSION_USER") => OwnerSpecification::session_user(),
            _ => {
                if let Some(name) = ast.name() {
                    if context.role_by_name(name).is_some() {
                        OwnerSpecification::role(&Role::from(name))
                    } else {
                        report(context, format!("role does not exist: {name}"))?;
                        return Ok(None);
                    }
                } else {
                    report(
                        context,
                        "PUBLIC is not allowed in this role position".to_owned(),
                    )?;
                    return Ok(None);
                }
            }
        };
        Ok(Some(Self::Role(specification)))
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
    ) -> Result<Option<Self>, AnalysisError> {
        let kind = ast.kind();
        let Some(object_kind) = kind.object_kind() else {
            report(
                context,
                format!("analysis catalog does not contain {kind:?} privilege targets"),
            )?;
            return Ok(None);
        };
        let mut grantees = Vec::new();
        for grantee in ast.grantees() {
            let Some(grantee) =
                PrivilegeRole::resolve(grantee, !ast.action().grants_with_grant_option(), context)?
            else {
                return Ok(None);
            };
            grantees.push(grantee);
        }
        let grantor = if let Some(grantor) = ast.grantor() {
            let Some(grantor) = PrivilegeRole::resolve(grantor, false, context)? else {
                return Ok(None);
            };
            Some(grantor)
        } else {
            None
        };
        let Some(targets) = Self::targets(ast.targets(), object_kind, context)? else {
            return Ok(None);
        };
        let Some(privileges) = Self::validate_privileges(ast, &targets, context)? else {
            return Ok(None);
        };
        Ok(Some(Self {
            targets,
            declaration: ObjectPrivilegeDeclaration {
                operation: ast.action().into(),
                privileges,
                grantees,
                grantor,
            },
        }))
    }

    fn validate_privileges(
        ast: &ObjectPrivileges,
        targets: &[OwnershipIdentity],
        context: &mut impl ParseContext,
    ) -> Result<Option<Vec<DeclaredPrivilege>>, AnalysisError> {
        let kind = ast.kind();
        let version = postgres_major(context);
        let mut privileges = Vec::new();
        for privilege in ast.privileges() {
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
                report(
                    context,
                    format!("privilege {} is invalid for {kind:?}", privilege.name()),
                )?;
                return Ok(None);
            }
            privileges.push(DeclaredPrivilege {
                name: privilege.name().to_owned(),
                columns: privilege.columns().to_vec(),
            });
        }
        for identity in targets {
            if matches!(identity.kind(), ObjectKind::TYPE | ObjectKind::DOMAIN)
                && context
                    .get_type_impl(identity.name())
                    .is_some_and(|definition| definition.is_array() == Some(true))
            {
                report(
                    context,
                    format!(
                        "cannot declare privileges on array type {}",
                        identity.name()
                    ),
                )?;
                return Ok(None);
            }
            for privilege in &privileges {
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
                        report(context, format!("column {column} does not exist on {name}"))?;
                        return Ok(None);
                    }
                }
            }
        }
        Ok(Some(privileges))
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

    fn targets(
        ast: &PrivilegeTargets,
        kind: ObjectKind,
        context: &mut impl ParseContext,
    ) -> Result<Option<Vec<OwnershipIdentity>>, AnalysisError> {
        let mut identities = Vec::new();
        match ast {
            PrivilegeTargets::Objects(objects) => {
                for target in objects {
                    if let PrivilegeTarget::Name(name) = target
                        && kind == ObjectKind::TABLE
                    {
                        let name = FullName::from_ast_name(name)?;
                        let Some(identity) = Self::relation_target(&name, context) else {
                            report(context, format!("relation does not exist: {name}"))?;
                            return Ok(None);
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
                        report(context, outcome.diagnostic_message())?;
                        return Ok(None);
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
                        report(context, format!("schema does not exist: {schema}"))?;
                        return Ok(None);
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
        Ok(Some(identities))
    }
}

/// Validated role membership declaration, retained on each granted role.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct RoleMembershipDeclaration {
    /// GRANT or REVOKE and its drop behavior.
    pub operation: PrivilegeOperation,
    /// Exact names of the roles being granted or revoked.
    pub roles: Vec<String>,
    /// Member role specifications in declaration order.
    pub members: Vec<PrivilegeRole>,
    /// Explicit grantor, when supplied.
    pub grantor: Option<PrivilegeRole>,
    /// Explicit ADMIN, INHERIT and SET options.
    pub options: Vec<(String, bool)>,
}

impl RoleMembershipDeclaration {
    pub(crate) fn analyze(
        ast: &RoleMembership,
        context: &mut impl ParseContext,
    ) -> Result<Option<Self>, AnalysisError> {
        let mut roles = Vec::new();
        for role in ast.roles() {
            let name = role.name();
            if !role.columns().is_empty() || context.role_by_name(name).is_none() {
                report(context, format!("invalid granted role: {name}"))?;
                return Ok(None);
            }
            if !roles.iter().any(|role| role == name) {
                roles.push(name.to_owned());
            }
        }
        let mut members = Vec::new();
        for member in ast.members() {
            let Some(member) = PrivilegeRole::resolve(member, false, context)? else {
                return Ok(None);
            };
            members.push(member);
        }
        let grantor = if let Some(grantor) = ast.grantor() {
            let Some(grantor) = PrivilegeRole::resolve(grantor, false, context)? else {
                return Ok(None);
            };
            Some(grantor)
        } else {
            None
        };
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
                report(context, format!("invalid role membership option: {name}"))?;
                return Ok(None);
            }
            options.push((name.to_owned(), option.value()));
        }
        Ok(Some(Self {
            operation: ast.action().into(),
            roles,
            members,
            grantor,
            options,
        }))
    }
}
