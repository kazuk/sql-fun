//! Public inspection and editing entry points for utility statements.
use crate::ast::{DefaultPrivileges, ForeignTable, Statement, StatementNode};

impl Statement {
    /// Inspect ALTER DEFAULT PRIVILEGES syntax without analysis.
    pub const fn default_privileges(&self) -> Option<&DefaultPrivileges> {
        self.node.default_privileges_node()
    }
    /// Edit ALTER DEFAULT PRIVILEGES syntax; successful edits apply immediately.
    pub const fn default_privileges_mut(&mut self) -> Option<&mut DefaultPrivileges> {
        self.node.default_privileges_mut()
    }
    /// Inspect a foreign-table definition without analysis or LIKE expansion.
    pub const fn foreign_table(&self) -> Option<&ForeignTable> {
        self.node.foreign_table_node()
    }
    /// Edit foreign-table syntax. Borrowing or dropping this view makes no change.
    pub const fn foreign_table_mut(&mut self) -> Option<&mut ForeignTable> {
        self.node.foreign_table_mut()
    }
}
impl StatementNode {
    /// Inspect ALTER DEFAULT PRIVILEGES syntax without analysis.
    const fn default_privileges_node(&self) -> Option<&DefaultPrivileges> {
        if let Self::DefaultPrivileges(value) = self {
            Some(value)
        } else {
            None
        }
    }
    /// Edit ALTER DEFAULT PRIVILEGES syntax. Each successful edit applies immediately.
    const fn default_privileges_mut(&mut self) -> Option<&mut DefaultPrivileges> {
        if let Self::DefaultPrivileges(value) = self {
            Some(value)
        } else {
            None
        }
    }
    /// Inspect a foreign-table definition without analysis or LIKE expansion.
    const fn foreign_table_node(&self) -> Option<&ForeignTable> {
        if let Self::ForeignTable(value) = self {
            Some(value)
        } else {
            None
        }
    }
    /// Edit foreign-table syntax. Borrowing or dropping this view makes no change.
    const fn foreign_table_mut(&mut self) -> Option<&mut ForeignTable> {
        if let Self::ForeignTable(value) = self {
            Some(value)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod utility_edit_tests {
    use crate::{
        ast::{Dialect, PrivilegeObject},
        parse_statements,
        test_helpers::assert_postgres_structure,
    };

    #[test]
    fn server_edits_preserve_the_owner_after_failure_abandonment_and_retry() {
        let original = r#"CREATE FOREIGN TABLE f (LIKE missing INCLUDING ALL EXCLUDING INDEXES, x int OPTIONS (remote 'x') DEFAULT 7) SERVER old OPTIONS (remote 'f')"#;
        let mut statement = parse_statements(original, Dialect::postgres())
            .next()
            .unwrap()
            .unwrap();
        let before = statement.deparse_sql(Dialect::postgres()).unwrap();
        {
            let _view = statement.foreign_table_mut().unwrap();
        }
        assert_eq!(statement.deparse_sql(Dialect::postgres()).unwrap(), before);
        assert!(
            statement
                .foreign_table_mut()
                .unwrap()
                .set_server("")
                .unwrap_err()
                .is_invalid_identifier()
        );
        assert_eq!(statement.deparse_sql(Dialect::postgres()).unwrap(), before);
        statement
            .foreign_table_mut()
            .unwrap()
            .set_server("Changed Server")
            .unwrap();
        let changed = statement.deparse_sql(Dialect::postgres()).unwrap();
        assert_eq!(changed, before.replace("\"old\"", "\"Changed Server\""));
        assert!(
            statement
                .foreign_table_mut()
                .unwrap()
                .set_server("bad\0name")
                .is_err()
        );
        assert_eq!(
            statement.foreign_table().unwrap().server(),
            "Changed Server"
        );
        assert_postgres_structure(&changed);
    }

    #[test]
    fn privilege_edits_preserve_role_and_schema_clauses_and_revoke_behavior() {
        let mut statement = parse_statements("ALTER DEFAULT PRIVILEGES FOR ROLE owner IN SCHEMA missing REVOKE SELECT ON TABLES FROM reader, PUBLIC CASCADE", Dialect::postgres()).next().unwrap().unwrap();
        let before = statement.deparse_sql(Dialect::postgres()).unwrap();
        {
            let value = statement.default_privileges_mut().unwrap();
            value.set_object(PrivilegeObject::large_objects());
            value.set_grant_option(true);
        }
        let after = statement.deparse_sql(Dialect::postgres()).unwrap();
        assert_eq!(
            after,
            before
                .replace("REVOKE ", "REVOKE GRANT OPTION FOR ")
                .replace("ON TABLES", "ON LARGE OBJECTS")
        );
        assert_postgres_structure(&after);
        assert!(statement.default_privileges().unwrap().cascade());
    }
}
