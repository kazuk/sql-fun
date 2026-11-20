//! Unresolved object privilege and role membership statements.

use crate::{
    ast::{
        DropBehavior, Name, ObjectTarget, PrivilegeGrantee, PrivilegeSpecification,
        TryFromWithTokens, postgres::required,
    },
    error::AnalysisError,
    syn::{self, ListOpt, Opt, ScanToken},
};
use ::sql_fun_core::IVec;

#[derive(Clone, Debug)]
pub(crate) struct ObjectPrivileges {
    action: PrivilegeAction,
    kind: PrivilegeTargetKind,
    targets: PrivilegeTargets,
    privileges: Vec<PrivilegeSpecification>,
    grantees: Vec<PrivilegeGrantee>,
    grantor: Option<PrivilegeGrantee>,
}

impl ObjectPrivileges {
    pub(crate) const fn is_grant(&self) -> bool {
        self.action.is_grant()
    }

    pub(crate) const fn command(&self) -> &'static str {
        self.action.command()
    }
}

#[derive(Clone, Debug)]
enum PrivilegeTargets {
    Objects(Vec<PrivilegeTarget>),
    AllInSchemas(Vec<String>),
}

#[derive(Clone, Debug)]
enum PrivilegeTarget {
    Name(Name),
    Routine(Box<ObjectTarget>),
    LargeObjectId(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PrivilegeTargetKind {
    Table,
    Sequence,
    Database,
    Domain,
    ForeignDataWrapper,
    ForeignServer,
    Function,
    Procedure,
    Routine,
    Language,
    LargeObject,
    Parameter,
    Schema,
    Tablespace,
    Type,
}

impl TryFrom<Option<syn::ObjectType>> for PrivilegeTargetKind {
    type Error = AnalysisError;

    fn try_from(value: Option<syn::ObjectType>) -> Result<Self, Self::Error> {
        use syn::ObjectType as SynKind;
        Ok(match value {
            Some(SynKind::ObjectTable) => Self::Table,
            Some(SynKind::ObjectSequence) => Self::Sequence,
            Some(SynKind::ObjectDatabase) => Self::Database,
            Some(SynKind::ObjectDomain) => Self::Domain,
            Some(SynKind::ObjectFdw) => Self::ForeignDataWrapper,
            Some(SynKind::ObjectForeignServer) => Self::ForeignServer,
            Some(SynKind::ObjectFunction) => Self::Function,
            Some(SynKind::ObjectProcedure) => Self::Procedure,
            Some(SynKind::ObjectRoutine) => Self::Routine,
            Some(SynKind::ObjectLanguage) => Self::Language,
            Some(SynKind::ObjectLargeobject) => Self::LargeObject,
            Some(SynKind::ObjectParameterAcl) => Self::Parameter,
            Some(SynKind::ObjectSchema) => Self::Schema,
            Some(SynKind::ObjectTablespace) => Self::Tablespace,
            Some(SynKind::ObjectType) => Self::Type,
            _ => return AnalysisError::raise_unexpected_input("privilege target object kind"),
        })
    }
}

impl PrivilegeTarget {
    fn try_from_node(
        value: &syn::Node,
        kind: PrivilegeTargetKind,
        tokens: &IVec<ScanToken>,
    ) -> Result<Self, AnalysisError> {
        match kind {
            PrivilegeTargetKind::Table | PrivilegeTargetKind::Sequence => {
                let relation = required(value.as_range_var().as_inner(), "privilege relation")?;
                Ok(Self::Name(Name::try_from(&relation)?))
            }
            PrivilegeTargetKind::Domain | PrivilegeTargetKind::Type => {
                Ok(Self::Name(Name::try_from(value.as_list().get_items())?))
            }
            PrivilegeTargetKind::Function
            | PrivilegeTargetKind::Procedure
            | PrivilegeTargetKind::Routine => Ok(Self::Routine(Box::new(
                ObjectTarget::try_from_node(value, tokens)?,
            ))),
            PrivilegeTargetKind::LargeObject => {
                // PostgreSQL puts unsigned OIDs above i32::MAX in Float nodes.
                let id = if let Some(integer) = value.as_integer().get_ival() {
                    integer.to_string()
                } else {
                    required(value.as_float().get_fval(), "large object ID")?
                };
                Ok(Self::LargeObjectId(id))
            }
            _ => {
                let name = required(value.as_string().get_sval(), "privilege object name")?;
                Ok(Self::Name(Name(vec![name])))
            }
        }
    }
}

impl TryFromWithTokens<&syn::GrantStmt> for ObjectPrivileges {
    type Error = AnalysisError;

    fn try_from_with_tokens(
        value: &syn::GrantStmt,
        tokens: &IVec<ScanToken>,
    ) -> Result<Self, Self::Error> {
        let kind = PrivilegeTargetKind::try_from(value.get_objtype().as_inner())?;
        let targets = match value.get_targtype().as_inner() {
            Some(syn::GrantTargetType::AclTargetAllInSchema) => {
                PrivilegeTargets::AllInSchemas(Name::try_from(value.get_objects())?.0)
            }
            Some(syn::GrantTargetType::AclTargetObject) => {
                let mut targets = Vec::new();
                for node in value.get_objects().as_inner().unwrap_or_default() {
                    targets.push(PrivilegeTarget::try_from_node(&node, kind, tokens)?);
                }
                PrivilegeTargets::Objects(targets)
            }
            _ => return AnalysisError::raise_unexpected_input("privilege target scope"),
        };
        Ok(Self {
            action: PrivilegeAction::try_from_parts(
                value.get_is_grant(),
                value.get_grant_option(),
                value.get_behavior().as_inner(),
            )?,
            kind,
            targets,
            privileges: PrivilegeSpecification::collect(value.get_privileges())?,
            grantees: PrivilegeGrantee::collect(value.get_grantees())?,
            grantor: value
                .get_grantor()
                .as_inner()
                .as_ref()
                .map(PrivilegeGrantee::try_from)
                .transpose()?,
        })
    }
}

#[cfg(test)]
mod object_privilege_conversion_tests {
    use crate::{
        ast::{
            Dialect, DropBehavior, StatementNode,
            privileges::{PrivilegeTarget, PrivilegeTargetKind, PrivilegeTargets},
        },
        parse_statements,
    };

    #[rstest::rstest]
    #[case(
        "GRANT SELECT ON pg_stat_statements TO PUBLIC",
        PrivilegeTargetKind::Table
    )]
    #[case(
        "REVOKE ALL ON FUNCTION bt_index_check(regclass) FROM PUBLIC",
        PrivilegeTargetKind::Function
    )]
    #[case(
        "REVOKE ALL ON FUNCTION dblink_connect_u(text) FROM PUBLIC",
        PrivilegeTargetKind::Function
    )]
    #[case(
        "GRANT USAGE ON SEQUENCE app.s TO PUBLIC",
        PrivilegeTargetKind::Sequence
    )]
    #[case("GRANT TEMP ON DATABASE db TO PUBLIC", PrivilegeTargetKind::Database)]
    #[case("GRANT USAGE ON DOMAIN app.d TO PUBLIC", PrivilegeTargetKind::Domain)]
    #[case(
        "GRANT USAGE ON FOREIGN DATA WRAPPER fdw TO PUBLIC",
        PrivilegeTargetKind::ForeignDataWrapper
    )]
    #[case(
        "GRANT USAGE ON FOREIGN SERVER srv TO PUBLIC",
        PrivilegeTargetKind::ForeignServer
    )]
    #[case(
        "GRANT EXECUTE ON PROCEDURE app.p(IN a int, OUT b text) TO PUBLIC",
        PrivilegeTargetKind::Procedure
    )]
    #[case(
        "GRANT EXECUTE ON ROUTINE app.p TO PUBLIC",
        PrivilegeTargetKind::Routine
    )]
    #[case("GRANT USAGE ON LANGUAGE sql TO PUBLIC", PrivilegeTargetKind::Language)]
    #[case(
        "GRANT SELECT ON LARGE OBJECT 4294967295 TO PUBLIC",
        PrivilegeTargetKind::LargeObject
    )]
    #[case(
        "GRANT SET, ALTER SYSTEM ON PARAMETER app.setting TO CURRENT_USER",
        PrivilegeTargetKind::Parameter
    )]
    #[case("GRANT USAGE ON SCHEMA app TO PUBLIC", PrivilegeTargetKind::Schema)]
    #[case(
        "GRANT CREATE ON TABLESPACE ts TO PUBLIC",
        PrivilegeTargetKind::Tablespace
    )]
    #[case("GRANT USAGE ON TYPE app.t TO PUBLIC", PrivilegeTargetKind::Type)]
    fn converts_object_targets(#[case] sql: &str, #[case] kind: PrivilegeTargetKind) {
        let statement = parse_statements(sql, Dialect::postgres())
            .next()
            .unwrap()
            .unwrap();
        let StatementNode::ObjectPrivileges(value) = statement.node() else {
            panic!("expected object privileges");
        };
        assert_eq!(value.kind, kind);
        let PrivilegeTargets::Objects(targets) = &value.targets else {
            panic!("expected explicit targets");
        };
        assert_eq!(targets.len(), 1);
        match (&targets[0], kind) {
            (PrivilegeTarget::LargeObjectId(id), PrivilegeTargetKind::LargeObject) => {
                assert_eq!(id, "4294967295");
            }
            (PrivilegeTarget::Name(name), PrivilegeTargetKind::Parameter) => {
                assert_eq!(name.0, ["app.setting"]);
                assert_eq!(value.privileges[0].name(), "set");
                assert_eq!(value.privileges[1].name(), "alter system");
            }
            (PrivilegeTarget::Name(name), PrivilegeTargetKind::Type) => {
                assert_eq!(name.0, ["app", "t"]);
            }
            _ => {}
        }
    }

    #[test]
    fn retains_scope_and_revoke_options() {
        let statement = parse_statements(
            r#"REVOKE GRANT OPTION FOR ALL ON ALL PROCEDURES IN SCHEMA "App", public FROM "Public", PUBLIC GRANTED BY CURRENT_USER CASCADE"#,
            Dialect::postgres(),
        ).next().unwrap().unwrap();
        let StatementNode::ObjectPrivileges(value) = statement.node() else {
            panic!("expected object privileges");
        };
        assert_eq!(value.kind, PrivilegeTargetKind::Procedure);
        let PrivilegeTargets::AllInSchemas(names) = &value.targets else {
            panic!("expected schema-wide targets");
        };
        assert_eq!(names, &["App", "public"]);
        assert!(value.action.revokes_grant_option_only());
        assert_eq!(value.action.revoke_behavior(), Some(DropBehavior::Cascade));
        assert!(value.privileges.is_empty());
        assert_eq!(value.grantees[0].name(), Some("Public"));
        assert_eq!(value.grantees[1].keyword(), Some("PUBLIC"));
        assert_eq!(
            value.grantor.as_ref().unwrap().keyword(),
            Some("CURRENT_USER")
        );
    }

    #[test]
    fn retains_column_privileges_and_routine_signatures() {
        let statement = parse_statements(
            "GRANT ALL (a,b) ON app.t TO r WITH GRANT OPTION",
            Dialect::postgres(),
        )
        .next()
        .unwrap()
        .unwrap();
        let StatementNode::ObjectPrivileges(value) = statement.node() else {
            panic!("expected object privileges");
        };
        assert!(value.action.grants_with_grant_option());
        assert_eq!(value.privileges[0].name(), "");
        assert_eq!(value.privileges[0].columns(), ["a", "b"]);

        let statement = parse_statements(
            "GRANT EXECUTE ON PROCEDURE app.p, app.p(), app.p(OUT a text, IN b int) TO r",
            Dialect::postgres(),
        )
        .next()
        .unwrap()
        .unwrap();
        let StatementNode::ObjectPrivileges(value) = statement.node() else {
            panic!("expected object privileges");
        };
        let PrivilegeTargets::Objects(targets) = &value.targets else {
            panic!("expected explicit targets");
        };
        for (target, expected) in targets.iter().zip([None, Some(0), Some(2)]) {
            let PrivilegeTarget::Routine(routine) = target else {
                panic!("expected routine");
            };
            assert_eq!(routine.name, ["app", "p"]);
            assert_eq!(routine.argument_types.as_ref().map(Vec::len), expected);
            if expected == Some(2) {
                let arguments = routine.argument_types.as_ref().unwrap();
                assert_eq!(arguments[0].mode.as_deref(), Some("OUT"));
                assert_eq!(arguments[0].name.as_deref(), Some("a"));
                assert_eq!(arguments[0].type_name.as_ref().unwrap().name.0, ["text"]);
                assert_eq!(arguments[1].mode.as_deref(), Some("IN"));
                assert_eq!(arguments[1].name.as_deref(), Some("b"));
                assert_eq!(
                    arguments[1].type_name.as_ref().unwrap().name.0,
                    ["pg_catalog", "int4"]
                );
            }
        }
    }
}

#[derive(Clone, Debug)]
enum PrivilegeAction {
    Grant(GrantPrivilegeAction),
    Revoke(RevokePrivilegeAction),
}

#[derive(Clone, Debug)]
struct GrantPrivilegeAction {
    grant_option: bool,
}

#[derive(Clone, Debug)]
struct RevokePrivilegeAction {
    grant_option_only: bool,
    behavior: DropBehavior,
}

impl PrivilegeAction {
    const fn grant(grant_option: bool) -> Self {
        Self::Grant(GrantPrivilegeAction { grant_option })
    }

    const fn revoke(grant_option_only: bool, behavior: DropBehavior) -> Self {
        Self::Revoke(RevokePrivilegeAction {
            grant_option_only,
            behavior,
        })
    }

    fn try_from_parts(
        is_grant: bool,
        grant_option: bool,
        behavior: Option<syn::DropBehavior>,
    ) -> Result<Self, AnalysisError> {
        if is_grant {
            Ok(Self::grant(grant_option))
        } else {
            Ok(Self::revoke(
                grant_option,
                DropBehavior::try_from(behavior)?,
            ))
        }
    }

    const fn is_grant(&self) -> bool {
        matches!(self, Self::Grant(_))
    }

    const fn grants_with_grant_option(&self) -> bool {
        match self {
            Self::Grant(grant) => grant.grant_option,
            Self::Revoke(_) => false,
        }
    }

    const fn revokes_grant_option_only(&self) -> bool {
        match self {
            Self::Grant(_) => false,
            Self::Revoke(revoke) => revoke.grant_option_only,
        }
    }

    const fn revoke_behavior(&self) -> Option<DropBehavior> {
        match self {
            Self::Grant(_) => None,
            Self::Revoke(revoke) => Some(revoke.behavior),
        }
    }

    const fn command(&self) -> &'static str {
        if self.is_grant() { "GRANT" } else { "REVOKE" }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct RoleMembership {
    action: PrivilegeAction,
    roles: Vec<PrivilegeSpecification>,
    members: Vec<PrivilegeGrantee>,
    grantor: Option<PrivilegeGrantee>,
    options: Vec<MembershipOption>,
}

impl RoleMembership {
    pub(crate) const fn is_grant(&self) -> bool {
        self.action.is_grant()
    }

    pub(crate) const fn command(&self) -> &'static str {
        self.action.command()
    }
}

#[derive(Clone, Debug)]
struct MembershipOption {
    name: String,
    value: bool,
}

impl TryFrom<&syn::GrantRoleStmt> for RoleMembership {
    type Error = AnalysisError;

    fn try_from(value: &syn::GrantRoleStmt) -> Result<Self, Self::Error> {
        let mut options = Vec::new();
        for node in value.get_opt().as_inner().unwrap_or_default() {
            let option = required(node.as_def_elem().as_inner(), "membership option")?;
            let arg = required(option.get_arg().as_inner(), "membership option value")?;
            options.push(MembershipOption {
                name: option.get_defname(),
                value: required(arg.as_boolean().get_boolval(), "membership boolean")?,
            });
        }
        Ok(Self {
            action: PrivilegeAction::try_from_parts(
                value.get_is_grant(),
                false,
                value.get_behavior().as_inner(),
            )?,
            roles: PrivilegeSpecification::collect(value.get_granted_roles())?,
            members: PrivilegeGrantee::collect(value.get_grantee_roles())?,
            grantor: value
                .get_grantor()
                .as_inner()
                .as_ref()
                .map(PrivilegeGrantee::try_from)
                .transpose()?,
            options,
        })
    }
}

#[cfg(test)]
mod membership_conversion_tests {
    use crate::{
        ast::{Dialect, DropBehavior, StatementNode},
        parse_statements,
    };

    #[rstest::rstest]
    #[case(
        "GRANT readers, writers TO CURRENT_ROLE, SESSION_USER WITH ADMIN OPTION, INHERIT FALSE, SET TRUE GRANTED BY CURRENT_USER",
        true
    )]
    #[case(
        "REVOKE SET OPTION FOR readers, writers FROM CURRENT_ROLE, SESSION_USER GRANTED BY CURRENT_USER CASCADE",
        false
    )]
    fn retains_role_membership_options(#[case] sql: &str, #[case] grant: bool) {
        let statement = parse_statements(sql, Dialect::postgres())
            .next()
            .unwrap()
            .unwrap();
        let StatementNode::RoleMembership(value) = statement.node() else {
            panic!("expected role membership");
        };
        assert_eq!(value.is_grant(), grant);
        assert_eq!(value.roles[0].name(), "readers");
        assert_eq!(value.roles[1].name(), "writers");
        assert_eq!(value.members[0].keyword(), Some("CURRENT_ROLE"));
        assert_eq!(value.members[1].keyword(), Some("SESSION_USER"));
        assert_eq!(
            value.grantor.as_ref().unwrap().keyword(),
            Some("CURRENT_USER")
        );
        let options: Vec<_> = value
            .options
            .iter()
            .map(|option| (option.name.as_str(), option.value))
            .collect();
        if grant {
            assert_eq!(
                options,
                [("admin", true), ("inherit", false), ("set", true)]
            );
        } else {
            assert_eq!(options, [("set", false)]);
            assert_eq!(value.action.revoke_behavior(), Some(DropBehavior::Cascade));
        }
    }
}
