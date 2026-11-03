//! Unresolved ALTER DEFAULT PRIVILEGES syntax and conversion.
use crate::{
    ast::{
        Name, Role, TryFromWithTokens,
        deparse::{DeparseError, SqlEmit, SqlWriter},
        postgres::required,
    },
    context::ParseContext,
    engine_specific::{Postgres18Feature, Postgres18Syntax},
    sem::AnalysisError,
    syn::{self, ListOpt, Opt},
};
use ::sql_fun_core::IVec;

/// PostgreSQL object category in an ALTER DEFAULT PRIVILEGES action.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PrivilegeObject {
    /// Tables and related relations.
    Tables,
    /// Sequences.
    Sequences,
    /// Functions, including the ROUTINES spelling normalized by PostgreSQL.
    Functions,
    /// Types.
    Types,
    /// Schemas.
    Schemas,
    /// Large objects (PostgreSQL 18 and later).
    LargeObjects,
}

impl PrivilegeObject {
    /// Table privileges.
    pub const fn tables() -> Self {
        Self::Tables
    }
    /// Sequence privileges.
    pub const fn sequences() -> Self {
        Self::Sequences
    }
    /// Function/routine privileges.
    pub const fn functions() -> Self {
        Self::Functions
    }
    /// Type privileges.
    pub const fn types() -> Self {
        Self::Types
    }
    /// Schema privileges.
    pub const fn schemas() -> Self {
        Self::Schemas
    }
    /// Large-object privileges.
    pub const fn large_objects() -> Self {
        Self::LargeObjects
    }

    const fn sql(self) -> &'static str {
        match self {
            Self::Tables => "TABLES",
            Self::Sequences => "SEQUENCES",
            Self::Functions => "FUNCTIONS",
            Self::Types => "TYPES",
            Self::Schemas => "SCHEMAS",
            Self::LargeObjects => "LARGE OBJECTS",
        }
    }
}

impl TryFrom<syn::ObjectType> for PrivilegeObject {
    type Error = AnalysisError;
    fn try_from(value: syn::ObjectType) -> Result<Self, Self::Error> {
        match value {
            syn::ObjectType::ObjectTable => Ok(Self::tables()),
            syn::ObjectType::ObjectSequence => Ok(Self::sequences()),
            syn::ObjectType::ObjectFunction => Ok(Self::functions()),
            syn::ObjectType::ObjectType => Ok(Self::types()),
            syn::ObjectType::ObjectSchema => Ok(Self::schemas()),
            syn::ObjectType::ObjectLargeobject => Ok(Self::large_objects()),
            _ => AnalysisError::raise_unexpected_input("default privilege object category"),
        }
    }
}

/// One ordered FOR ROLE or IN SCHEMA clause, with unresolved identifier spellings.
#[derive(Clone, Debug)]
pub enum DefaultPrivilegeOption {
    /// FOR ROLE/USER (normalized to FOR ROLE), including special role keywords.
    Roles(Vec<PrivilegeGrantee>),
    /// IN SCHEMA.
    Schemas(Vec<String>),
}
impl DefaultPrivilegeOption {
    const fn for_roles(roles: Vec<PrivilegeGrantee>) -> Self {
        Self::Roles(roles)
    }
    fn in_schemas(names: &[String]) -> Self {
        Self::Schemas(names.to_vec())
    }
    /// Role specifications for a FOR ROLE clause; None for IN SCHEMA.
    pub fn roles(&self) -> Option<&[PrivilegeGrantee]> {
        if let Self::Roles(values) = self {
            Some(values)
        } else {
            None
        }
    }
    /// Schema identifier spellings for an IN SCHEMA clause; None for FOR ROLE.
    pub fn schemas(&self) -> Option<&[String]> {
        if let Self::Schemas(values) = self {
            Some(values)
        } else {
            None
        }
    }
}
impl TryFrom<&syn::DefElem> for DefaultPrivilegeOption {
    type Error = AnalysisError;
    fn try_from(value: &syn::DefElem) -> Result<Self, Self::Error> {
        match value.get_defname().as_str() {
            "roles" => Ok(Self::for_roles(PrivilegeGrantee::collect(
                value.get_arg().as_list().get_items(),
            )?)),
            "schemas" => Ok(Self::in_schemas(
                &Name::try_from(value.get_arg().as_list().get_items())?.0,
            )),
            _ => AnalysisError::raise_unexpected_input("default privilege option"),
        }
    }
}

/// A privilege name and optional column-name list, retained without ACL validation.
#[derive(Clone, Debug)]
pub struct PrivilegeSpecification {
    name: String,
    columns: Vec<String>,
}
impl PrivilegeSpecification {
    /// Privilege spelling normalized by the PostgreSQL parser.
    /// Empty denotes column-qualified ALL, whose columns are retained separately.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Column identifiers in source order, when supplied by the parser.
    pub fn columns(&self) -> &[String] {
        &self.columns
    }
}
impl TryFrom<&syn::AccessPriv> for PrivilegeSpecification {
    type Error = AnalysisError;
    fn try_from(value: &syn::AccessPriv) -> Result<Self, Self::Error> {
        Ok(Self {
            name: value.get_priv_name(),
            columns: Name::try_from(value.get_cols())?.0,
        })
    }
}

/// A syntactic grantee, without a role-catalog binding.
#[derive(Clone, Debug)]
pub struct PrivilegeGrantee {
    role: Role,
}
impl PrivilegeGrantee {
    fn collect(nodes: syn::NodeListOpt) -> Result<Vec<Self>, AnalysisError> {
        let mut values = Vec::new();
        for node in nodes.as_inner().unwrap_or_default() {
            values.push(Self::try_from(&required(
                node.as_role_spec().as_inner(),
                "privilege role specification",
            )?)?);
        }
        Ok(values)
    }
    /// Named-role identifier, or None for a special role keyword.
    pub fn name(&self) -> Option<&str> {
        if self.role.kind == 1 {
            Some(&self.role.name)
        } else {
            None
        }
    }
    /// Special role keyword, or None for an ordinary named role.
    pub const fn keyword(&self) -> Option<&'static str> {
        match self.role.kind {
            2 => Some("CURRENT_ROLE"),
            3 => Some("CURRENT_USER"),
            4 => Some("SESSION_USER"),
            5 => Some("PUBLIC"),
            _ => None,
        }
    }
}
impl TryFrom<&syn::RoleSpec> for PrivilegeGrantee {
    type Error = AnalysisError;
    fn try_from(value: &syn::RoleSpec) -> Result<Self, Self::Error> {
        Ok(Self {
            role: Role::try_from(value)?,
        })
    }
}

/// Structured ALTER DEFAULT PRIVILEGES syntax. No reference has been resolved.
#[derive(Clone, Debug)]
pub struct DefaultPrivileges {
    options: Vec<DefaultPrivilegeOption>,
    is_grant: bool,
    object: PrivilegeObject,
    privileges: Vec<PrivilegeSpecification>,
    grantees: Vec<PrivilegeGrantee>,
    grant_option: bool,
    cascade: bool,
}
impl DefaultPrivileges {
    /// Ordered target-role/schema clauses.
    pub fn options(&self) -> &[DefaultPrivilegeOption] {
        &self.options
    }
    /// Whether the action grants rather than revokes privileges.
    pub const fn is_grant(&self) -> bool {
        self.is_grant
    }
    /// Target object category.
    pub const fn object(&self) -> PrivilegeObject {
        self.object
    }
    /// Explicit privileges; an empty list represents ALL PRIVILEGES.
    pub fn privileges(&self) -> &[PrivilegeSpecification] {
        &self.privileges
    }
    /// Whether ALL PRIVILEGES was requested.
    pub const fn is_all_privileges(&self) -> bool {
        self.privileges.is_empty()
    }
    /// Grantees in source order, retaining PUBLIC and special role keywords.
    pub fn grantees(&self) -> &[PrivilegeGrantee] {
        &self.grantees
    }
    /// Whether a grant option is granted or revoked.
    pub const fn grant_option(&self) -> bool {
        self.grant_option
    }
    /// Whether revocation uses CASCADE rather than RESTRICT.
    pub const fn cascade(&self) -> bool {
        self.cascade
    }
    /// Change the syntactic object category immediately, preserving all other attributes.
    /// No privilege/category compatibility validation or name resolution is performed.
    pub const fn set_object(&mut self, object: PrivilegeObject) {
        self.object = object;
    }
    /// Set WITH GRANT OPTION (GRANT) or GRANT OPTION FOR (REVOKE) immediately.
    pub const fn set_grant_option(&mut self, enabled: bool) {
        self.grant_option = enabled;
    }

    pub(super) fn check_engine_features(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<bool, AnalysisError> {
        if self.object == PrivilegeObject::large_objects() {
            context.postgres18_syntax_used(Postgres18Syntax::LargeObjectDefaultPrivileges)
        } else {
            Ok(true)
        }
    }
}
impl TryFromWithTokens<&syn::AlterDefaultPrivilegesStmt> for DefaultPrivileges {
    type Error = AnalysisError;
    fn try_from_with_tokens(
        value: &syn::AlterDefaultPrivilegesStmt,
        _tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        let mut options = Vec::new();
        for node in value.get_options().as_inner().unwrap_or_default() {
            options.push(DefaultPrivilegeOption::try_from(&required(
                node.as_def_elem().as_inner(),
                "default privilege option",
            )?)?);
        }
        let action = required(value.get_action().as_inner(), "default privilege action")?;
        let mut privileges = Vec::new();
        for node in action.get_privileges().as_inner().unwrap_or_default() {
            privileges.push(PrivilegeSpecification::try_from(&required(
                node.as_access_priv().as_inner(),
                "privilege specification",
            )?)?);
        }
        let grantees = PrivilegeGrantee::collect(action.get_grantees())?;
        Ok(Self {
            options,
            privileges,
            grantees,
            is_grant: action.get_is_grant(),
            object: PrivilegeObject::try_from(required(
                action.get_objtype().as_inner(),
                "privilege object category",
            )?)?,
            grant_option: action.get_grant_option(),
            cascade: matches!(
                action.get_behavior().as_inner(),
                Some(syn::DropBehavior::DropCascade)
            ),
        })
    }
}

#[cfg(test)]
mod default_privilege_conversion_tests {
    use crate::{
        ast::{Dialect, PrivilegeObject},
        parse_statements,
        test_helpers::assert_postgres_structure,
    };

    #[rstest::rstest]
    #[case("ALTER DEFAULT PRIVILEGES GRANT SELECT, UPDATE ON TABLES TO PUBLIC WITH GRANT OPTION")]
    #[case(r#"ALTER DEFAULT PRIVILEGES FOR USER "Owner", other IN SCHEMA "App", public GRANT USAGE ON SEQUENCES TO GROUP "Reader", PUBLIC"#)]
    #[case(
        "ALTER DEFAULT PRIVILEGES IN SCHEMA a FOR ROLE r IN SCHEMA b REVOKE ALL PRIVILEGES ON TABLES FROM PUBLIC CASCADE"
    )]
    #[case(
        "ALTER DEFAULT PRIVILEGES REVOKE GRANT OPTION FOR EXECUTE ON ROUTINES FROM PUBLIC RESTRICT"
    )]
    #[case(
        "ALTER DEFAULT PRIVILEGES GRANT EXECUTE ON FUNCTIONS TO CURRENT_USER, CURRENT_ROLE, SESSION_USER"
    )]
    #[case("ALTER DEFAULT PRIVILEGES GRANT USAGE ON TYPES TO PUBLIC")]
    #[case("ALTER DEFAULT PRIVILEGES GRANT CREATE ON SCHEMAS TO PUBLIC")]
    #[case("ALTER DEFAULT PRIVILEGES GRANT SELECT, UPDATE ON LARGE OBJECTS TO PUBLIC")]
    #[case("ALTER DEFAULT PRIVILEGES REVOKE GRANT OPTION FOR ALL ON LARGE OBJECTS FROM r CASCADE")]
    #[case("ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT SELECT ON LARGE OBJECTS TO PUBLIC")]
    #[case("ALTER DEFAULT PRIVILEGES GRANT SELECT (a), UPDATE (b,c) ON TABLES TO PUBLIC")]
    #[case("ALTER DEFAULT PRIVILEGES GRANT ALL (a,b) ON TABLES TO PUBLIC")]
    fn native_tree_round_trip(#[case] sql: &str) {
        assert_postgres_structure(sql);
    }

    #[test]
    fn retains_unresolved_names_and_action_attributes() {
        let statement = parse_statements(r#"ALTER DEFAULT PRIVILEGES IN SCHEMA "Missing" FOR ROLE "Owner" REVOKE GRANT OPTION FOR SELECT, UPDATE ON LARGE OBJECTS FROM "Reader", PUBLIC CASCADE"#, Dialect::postgres()).next().unwrap().unwrap();
        let value = statement.default_privileges().unwrap();
        assert!(!value.is_grant());
        assert_eq!(value.object(), PrivilegeObject::large_objects());
        assert_eq!(value.options()[0].schemas().unwrap(), ["Missing"]);
        assert!(value.options()[0].roles().is_none());
        assert!(value.options()[1].schemas().is_none());
        assert_eq!(value.options()[1].roles().unwrap()[0].name(), Some("Owner"));
        assert_eq!(value.privileges()[0].name(), "select");
        assert_eq!(value.privileges()[1].name(), "update");
        assert_eq!(value.grantees()[0].name(), Some("Reader"));
        assert_eq!(value.grantees()[1].keyword(), Some("PUBLIC"));
        assert!(value.grant_option());
        assert!(value.cascade());
    }
}

impl SqlEmit for DefaultPrivileges {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        w.postgres_only("ALTER DEFAULT PRIVILEGES")?;
        w.sql("ALTER DEFAULT PRIVILEGES");
        for option in &self.options {
            option.emit(w)?;
        }
        w.sql(if self.is_grant { " GRANT " } else { " REVOKE " });
        if !self.is_grant && self.grant_option {
            w.sql("GRANT OPTION FOR ");
        }
        if self.privileges.is_empty() {
            w.sql("ALL PRIVILEGES");
        } else {
            w.list(&self.privileges, ", ")?;
        }
        w.sql(" ON ");
        w.sql(self.object.sql());
        w.sql(if self.is_grant { " TO " } else { " FROM " });
        w.list(&self.grantees, ", ")?;
        if self.is_grant && self.grant_option {
            w.sql(" WITH GRANT OPTION");
        }
        if !self.is_grant {
            w.sql(if self.cascade {
                " CASCADE"
            } else {
                " RESTRICT"
            });
        }
        Ok(())
    }
}
impl SqlEmit for DefaultPrivilegeOption {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        match self {
            Self::Roles(values) => {
                w.sql(" FOR ROLE ");
                w.list(values, ", ")
            }
            Self::Schemas(values) => {
                w.sql(" IN SCHEMA ");
                w.names(values, ", ")
            }
        }
    }
}
impl SqlEmit for PrivilegeGrantee {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        self.role.emit(w)
    }
}
impl SqlEmit for PrivilegeSpecification {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        if self.name.is_empty() {
            w.sql("ALL");
        } else {
            w.ident(&self.name)?;
        }
        if !self.columns.is_empty() {
            w.sql(" (");
            w.names(&self.columns, ", ")?;
            w.sql(")");
        }
        Ok(())
    }
}
