//! Supported inspection of the owned common AST.

#[cfg(all(feature = "sqlite", feature = "workaround_turso_9791"))]
mod sqlite_retention;
mod wildcard;

use crate::{
    StringSpan,
    ast::{
        Cte, CteBody, DefaultPrivileges, Dialect, ForeignTable, StatementNode, With,
        deparse::{DeparseError, FailureKind, SqlEmit, SqlWriter},
        mutation::{AstMutationError, CteDefinition, HaveWithClause, WithClauseMut},
        projection::ScopedSourceEdit,
        traversal::{AstVisitorMut, WalkSqlMut},
    },
};

/// An owned, unresolved statement in an explicitly selected SQL dialect.
///
/// Parsing preserves syntax without resolving names against a catalog. Private
/// nodes retain additional syntax even where no inspection method is exposed yet.
#[derive(Clone, Debug)]
pub struct Statement {
    node: StatementNode,
    dialect: Dialect,
    span: StringSpan,
}

impl Statement {
    pub(crate) const fn node(&self) -> &StatementNode {
        &self.node
    }

    #[cfg(test)]
    pub(crate) fn into_test_node(self) -> StatementNode {
        self.node
    }

    pub(crate) const fn from_node(node: StatementNode, dialect: Dialect, span: StringSpan) -> Self {
        Self {
            node,
            dialect,
            span,
        }
    }

    /// Range of this statement in the original input, in UTF-8 byte offsets.
    ///
    /// The range identifies the input supplied to `parse_statements`. It is
    /// retained when the statement is cloned or edited; it does not describe
    /// rendered SQL after AST mutations.
    pub const fn span(&self) -> &StringSpan {
        &self.span
    }

    /// Dialect explicitly chosen when this statement was parsed.
    pub const fn dialect(&self) -> Dialect {
        self.dialect
    }

    /// Kind of SQL statement, independent of the parser representation.
    pub const fn kind(&self) -> StatementKind {
        self.node.kind()
    }

    /// Whether this is a query (including VALUES and set operations).
    pub const fn is_query(&self) -> bool {
        self.node.is_query()
    }

    /// Whether this is an INSERT statement.
    pub const fn is_insert(&self) -> bool {
        self.node.is_insert()
    }

    /// Whether this is an UPDATE statement.
    pub const fn is_update(&self) -> bool {
        self.node.is_update()
    }

    /// Whether this is a DELETE statement.
    pub const fn is_delete(&self) -> bool {
        self.node.is_delete()
    }

    /// Whether this is a MERGE statement.
    pub const fn is_merge(&self) -> bool {
        self.node.is_merge()
    }
}

impl Statement {
    /// Inspect an existing WITH clause without exposing mutable node storage.
    pub const fn with_clause(&self) -> Option<WithClause<'_>> {
        if let Some(node) = self.node.with_node() {
            WithClause::existing(node, self.dialect)
        } else {
            None
        }
    }
}

/// Parser-independent statement category.
///
/// Trigger and rule statements have dedicated categories; the former aggregate
/// `ViewWrite` category has been removed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatementKind {
    /// An unresolved PostgreSQL DROP RULE statement. Analysis awaits #605.
    DropRule,
    /// An unresolved CREATE TRIGGER definition. Analysis awaits #602 / #555.
    CreateTrigger,
    /// A PostgreSQL CREATE RULE definition; analysis resolves its statement-specific semantics.
    CreateRule,
    /// An unresolved DROP TRIGGER statement. Analysis awaits #555.
    DropTrigger,
    /// SELECT, VALUES, or a set operation.
    Query,
    /// INSERT.
    Insert,
    /// UPDATE.
    Update,
    /// DELETE.
    Delete,
    /// MERGE.
    Merge,
    /// TRUNCATE; common-AST conversion may still be unavailable.
    Truncate,
    /// A view definition.
    View,
    /// An extension definition.
    Extension,
    /// A variable assignment.
    VariableSet,
    /// An enum type definition.
    EnumType,
    /// A sequence definition.
    Sequence,
    /// DROP TABLE.
    DropTable,
    /// ALTER SEQUENCE.
    AlterSequence,
    /// COMMENT.
    Comment,
    /// An ownership change.
    AlterOwner,
    /// A composite type definition.
    CompositeType,
    /// A domain definition.
    Domain,
    /// A function definition.
    Function,
    /// A schema definition.
    Schema,
    /// An index definition.
    Index,
    /// CREATE TABLE AS.
    TableAs,
    /// A table definition.
    Table,
    /// ALTER DEFAULT PRIVILEGES syntax; interpretation is separate.
    DefaultPrivileges,
    /// CREATE FOREIGN TABLE syntax; interpretation is separate.
    ForeignTable,
    /// ALTER TABLE or a related relation change.
    AlterRelation,
}

impl std::fmt::Display for StatementKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::DropRule => "DROP RULE",
            Self::CreateTrigger => "CREATE TRIGGER",
            Self::CreateRule => "CREATE RULE",
            Self::DropTrigger => "DROP TRIGGER",
            Self::Query => "query",
            Self::Insert => "INSERT",
            Self::Update => "UPDATE",
            Self::Delete => "DELETE",
            Self::Merge => "MERGE",
            Self::Truncate => "TRUNCATE",
            Self::View => "view definition",
            Self::Extension => "CREATE EXTENSION",
            Self::VariableSet => "configuration statement",
            Self::EnumType => "CREATE TYPE AS ENUM",
            Self::Sequence => "CREATE SEQUENCE",
            Self::DropTable => "DROP TABLE",
            Self::AlterSequence => "ALTER SEQUENCE",
            Self::Comment => "COMMENT",
            Self::AlterOwner => "ALTER OWNER",
            Self::CompositeType => "CREATE TYPE",
            Self::Domain => "CREATE DOMAIN",
            Self::Function => "function or procedure definition",
            Self::Schema => "CREATE SCHEMA",
            Self::Index => "CREATE INDEX",
            Self::TableAs => "query-backed relation definition",
            Self::Table => "CREATE TABLE",
            Self::DefaultPrivileges => "ALTER DEFAULT PRIVILEGES",
            Self::ForeignTable => "CREATE FOREIGN TABLE",
            Self::AlterRelation => "relation alteration",
        })
    }
}

impl StatementKind {
    /// The ALTER DEFAULT PRIVILEGES category.
    pub const fn default_privileges() -> Self {
        Self::DefaultPrivileges
    }
    /// The CREATE FOREIGN TABLE category.
    pub const fn foreign_table() -> Self {
        Self::ForeignTable
    }

    /// The TRUNCATE category, independently of common-AST conversion support.
    pub const fn truncate() -> Self {
        Self::Truncate
    }

    /// The CREATE TRIGGER category.
    pub const fn create_trigger() -> Self {
        Self::CreateTrigger
    }

    /// The PostgreSQL CREATE RULE category.
    pub const fn create_rule() -> Self {
        Self::CreateRule
    }

    /// The DROP TRIGGER category.
    pub const fn drop_trigger() -> Self {
        Self::DropTrigger
    }

    /// The PostgreSQL DROP RULE category.
    pub const fn drop_rule() -> Self {
        Self::DropRule
    }
    /// The query category.
    pub const fn query() -> Self {
        Self::Query
    }

    /// The insert category.
    pub const fn insert() -> Self {
        Self::Insert
    }

    /// The update category.
    pub const fn update() -> Self {
        Self::Update
    }

    /// The delete category.
    pub const fn delete() -> Self {
        Self::Delete
    }

    /// The merge category.
    pub const fn merge() -> Self {
        Self::Merge
    }

    /// The view category.
    pub const fn view() -> Self {
        Self::View
    }

    /// The extension category.
    pub const fn extension() -> Self {
        Self::Extension
    }

    /// The variable set category.
    pub const fn variable_set() -> Self {
        Self::VariableSet
    }

    /// The enum type category.
    pub const fn enum_type() -> Self {
        Self::EnumType
    }

    /// The sequence category.
    pub const fn sequence() -> Self {
        Self::Sequence
    }

    /// The drop table category.
    pub const fn drop_table() -> Self {
        Self::DropTable
    }

    /// The alter sequence category.
    pub const fn alter_sequence() -> Self {
        Self::AlterSequence
    }

    /// The comment category.
    pub const fn comment() -> Self {
        Self::Comment
    }

    /// The alter owner category.
    pub const fn alter_owner() -> Self {
        Self::AlterOwner
    }

    /// The composite type category.
    pub const fn composite_type() -> Self {
        Self::CompositeType
    }

    /// The domain category.
    pub const fn domain() -> Self {
        Self::Domain
    }

    /// The function category.
    pub const fn function() -> Self {
        Self::Function
    }

    /// The schema category.
    pub const fn schema() -> Self {
        Self::Schema
    }

    /// The index category.
    pub const fn index() -> Self {
        Self::Index
    }

    /// The table as category.
    pub const fn table_as() -> Self {
        Self::TableAs
    }

    /// The table category.
    pub const fn table() -> Self {
        Self::Table
    }

    /// The alter relation category.
    pub const fn alter_relation() -> Self {
        Self::AlterRelation
    }
}

impl StatementNode {
    pub(super) const fn kind(&self) -> StatementKind {
        match self {
            Self::Query(_) => StatementKind::query(),
            Self::Insert(_) => StatementKind::insert(),
            Self::Update(_) => StatementKind::update(),
            Self::Delete(_) => StatementKind::delete(),
            Self::Merge(_) => StatementKind::merge(),
            Self::View(_) => StatementKind::view(),
            Self::CreateTrigger(_) => StatementKind::create_trigger(),
            Self::CreateRule(_) => StatementKind::create_rule(),
            Self::DropTrigger(_) => StatementKind::drop_trigger(),
            Self::DropRule(_) => StatementKind::drop_rule(),
            Self::Extension(_) => StatementKind::extension(),
            Self::VariableSet(_) => StatementKind::variable_set(),
            Self::EnumType(_) => StatementKind::enum_type(),
            Self::Sequence(_) => StatementKind::sequence(),
            Self::DropTable(_) => StatementKind::drop_table(),
            Self::AlterSequence(_) => StatementKind::alter_sequence(),
            Self::Comment(_) => StatementKind::comment(),
            Self::AlterOwner(_) => StatementKind::alter_owner(),
            Self::CompositeType(_) => StatementKind::composite_type(),
            Self::Domain(_) => StatementKind::domain(),
            Self::Function(_) => StatementKind::function(),
            Self::Schema(_) => StatementKind::schema(),
            Self::Index(_) => StatementKind::index(),
            Self::TableAs(_) => StatementKind::table_as(),
            Self::Table(_) => StatementKind::table(),
            Self::DefaultPrivileges(_) => StatementKind::default_privileges(),
            Self::ForeignTable(_) => StatementKind::foreign_table(),
            Self::AlterRelation(_) => StatementKind::alter_relation(),
        }
    }

    const fn is_query(&self) -> bool {
        matches!(self, Self::Query(_))
    }

    const fn is_delete(&self) -> bool {
        matches!(self, Self::Delete(_))
    }

    const fn is_update(&self) -> bool {
        matches!(self, Self::Update(_))
    }

    const fn is_insert(&self) -> bool {
        matches!(self, Self::Insert(_))
    }

    const fn is_merge(&self) -> bool {
        matches!(self, Self::Merge(_))
    }

    pub(super) const fn with_node(&self) -> Option<&With> {
        match self {
            Self::Query(node) => Some(&node.with),
            Self::Insert(node) => Some(&node.with),
            Self::Update(node) => Some(&node.with),
            Self::Delete(node) => Some(&node.with),
            Self::Merge(node) => Some(node.with_node()),
            _ => None,
        }
    }
}

/// A read-only view of a WITH clause and its ordered CTE collection.
#[derive(Clone, Copy, Debug)]
pub struct WithClause<'a> {
    node: &'a With,
    dialect: Dialect,
}

impl<'a> WithClause<'a> {
    pub(in crate::ast) const fn from_node(node: &'a With, dialect: Dialect) -> Self {
        Self { node, dialect }
    }

    pub(super) const fn existing(node: &'a With, dialect: Dialect) -> Option<Self> {
        if node.entries.is_empty() {
            None
        } else {
            Some(Self { node, dialect })
        }
    }

    /// Whether WITH RECURSIVE was specified.
    pub const fn is_recursive(&self) -> bool {
        self.node.recursive
    }

    /// Number of CTEs in source order.
    pub const fn len(&self) -> usize {
        self.node.entries.len()
    }

    /// Whether the clause contains no CTEs.
    pub const fn is_empty(&self) -> bool {
        self.node.entries.is_empty()
    }

    /// Inspect CTEs in their source order.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = CommonTableExpression<'a>> + '_ {
        self.node.entries.iter().map(|node| CommonTableExpression {
            node,
            dialect: self.dialect,
        })
    }

    /// Find a CTE using its parsed identifier spelling.
    ///
    /// PostgreSQL spelling is already normalized by the parser. SQLite compares
    /// ASCII letters without regard to case, matching its identifier behavior.
    pub fn find(&self, name: &str) -> Option<CommonTableExpression<'a>> {
        self.node
            .entries
            .iter()
            .find(|node| self.dialect.same_identifier(&node.name, name))
            .map(|node| CommonTableExpression {
                node,
                dialect: self.dialect,
            })
    }
}

impl Dialect {
    pub(super) fn same_identifier(self, left: &str, right: &str) -> bool {
        if self == Self::sqlite() {
            #[cfg(feature = "sqlite")]
            {
                ::sql_fun_core::SqlDialect::SQLite.same_identifier(left, right)
            }
            // SQLite AST editing remains available without the SQLite parser.
            #[cfg(not(feature = "sqlite"))]
            {
                left.eq_ignore_ascii_case(right)
            }
        } else {
            ::sql_fun_core::SqlDialect::PostgreSQL.same_identifier(left, right)
        }
    }
}

/// A read-only CTE, retaining its declaration attributes and body.
#[derive(Clone, Copy, Debug)]
pub struct CommonTableExpression<'a> {
    node: &'a Cte,
    dialect: Dialect,
}

impl<'a> CommonTableExpression<'a> {
    pub(in crate::ast) const fn from_node(node: &'a Cte, dialect: Dialect) -> Self {
        Self { node, dialect }
    }

    /// Parsed CTE identifier spelling.
    pub fn name(&self) -> &'a str {
        &self.node.name
    }

    /// Declared column aliases in their original order.
    pub fn columns(&self) -> impl ExactSizeIterator<Item = &'a str> + '_ {
        self.node.columns.iter().map(|column| column.name.as_str())
    }

    /// Explicit materialization choice, or None when unspecified.
    pub const fn materialized(&self) -> Option<bool> {
        self.node.materialized
    }

    /// Whether a SEARCH clause is retained.
    pub const fn has_search(&self) -> bool {
        self.node.search.is_some()
    }

    /// Whether a CYCLE clause is retained.
    pub const fn has_cycle(&self) -> bool {
        self.node.cycle.is_some()
    }

    /// Kind of statement used as the CTE body.
    pub const fn body_kind(&self) -> StatementKind {
        self.node.body.kind()
    }

    /// Inspect a WITH clause belonging to the CTE body itself.
    pub const fn with_clause(&self) -> Option<WithClause<'a>> {
        WithClause::existing(self.node.body.with_node(), self.dialect)
    }
}

impl CteBody {
    pub(super) const fn kind(&self) -> StatementKind {
        match self {
            Self::Query(_) => StatementKind::query(),
            Self::Insert(_) => StatementKind::insert(),
            Self::Update(_) => StatementKind::update(),
            Self::Delete(_) => StatementKind::delete(),
            Self::Merge(_) => StatementKind::merge(),
        }
    }

    pub(super) const fn with_node(&self) -> &With {
        match self {
            Self::Query(node) => &node.with,
            Self::Insert(node) => &node.with,
            Self::Update(node) => &node.with,
            Self::Delete(node) => &node.with,
            Self::Merge(node) => node.with_node(),
        }
    }
}

impl Statement {
    /// Render the complete current syntax tree as one deterministic SQL statement.
    ///
    /// The output has no trailing semicolon. Ordinary comments and formatting are
    /// not retained; SQLite type trivia that affects declared types is preserved. The dialect must match the explicit parsing choice; this is
    /// not a dialect translation operation. Numbered parameter indices are never
    /// reassigned, including repeated indices and gaps. SQLite named and anonymous
    /// parameters use their stored numbered slots; bind the output by those slots.
    /// No schema is inspected.
    ///
    /// # Errors
    /// Returns [`DeparseError`] for a dialect mismatch, unavailable feature,
    /// invalid syntax/placement, or an unsupported represented node. The statement
    /// remains unchanged and partial output is never returned.
    pub fn deparse_sql(&self, dialect: Dialect) -> Result<String, DeparseError> {
        if dialect != self.dialect {
            return Err(DeparseError::new(
                dialect,
                "statement",
                "output dialect differs from the parsed statement",
                FailureKind::DialectMismatch,
            ));
        }
        #[cfg(not(feature = "sqlite"))]
        if dialect == Dialect::sqlite() {
            return Err(DeparseError::new(
                dialect,
                "statement",
                "enable the sqlite Cargo feature",
                FailureKind::UnavailableDialect,
            ));
        }
        let mut writer = SqlWriter::new(dialect);
        self.node.emit(&mut writer)?;
        Ok(writer.into_sql())
    }
}

#[cfg(test)]
mod test_deparse_errors {
    use crate::{
        ast::{
            ColumnConstraint, Dialect, Expr as AstExpr, ForeignKeyAction, Literal as AstLiteral,
            QueryBody, StatementNode,
        },
        parse_statements,
        test_helpers::deparse::statement,
    };

    #[test]
    fn reports_dialect_mismatch_without_modifying_statement() {
        let value = statement("select 1", Dialect::postgres());
        let original = value.deparse_sql(Dialect::postgres()).unwrap();
        let error = value.deparse_sql(Dialect::sqlite()).unwrap_err();
        assert!(error.is_dialect_mismatch());
        assert_eq!(error.dialect(), Dialect::sqlite());
        assert_eq!(original, value.deparse_sql(Dialect::postgres()).unwrap());
    }

    #[test]
    fn unsupported_node_is_structured_and_does_not_return_partial_sql() {
        let mut value = statement("alter table t add column x int", Dialect::postgres());
        let StatementNode::AlterRelation(relation) = &mut value.node else {
            panic!("alter fixture");
        };
        relation.commands = vec![crate::ast::AlterCommand::Unsupported(
            "future command".to_owned(),
        )];
        let before = format!("{value:?}");
        let error = value.deparse_sql(Dialect::postgres()).unwrap_err();
        assert!(error.is_unsupported());
        assert!(!error.is_invalid_syntax());
        assert!(error.node().contains("future command"));
        assert!(!error.message().is_empty());
        assert_eq!(format!("{value:?}"), before);
    }

    #[test]
    fn rejects_invalid_retained_literal_without_a_panic() {
        let mut value = statement("select 'valid'", Dialect::postgres());
        let StatementNode::Query(query) = &mut value.node else {
            panic!("query fixture");
        };
        let QueryBody::Select(select) = &mut query.body else {
            panic!("select fixture");
        };
        select.targets[0].expression = AstExpr::Literal(AstLiteral::String("\0".to_owned()));
        let error = value.deparse_sql(Dialect::postgres()).unwrap_err();
        assert!(error.is_invalid_syntax());
        assert!(!error.is_unsupported());
        assert!(error.node().contains("literal"));
    }

    #[cfg(feature = "sqlite")]
    #[test]
    fn rejects_turso_array_extension_as_invalid_sqlite_syntax() {
        let mut value = statement("select 1", Dialect::sqlite());
        let StatementNode::Query(query) = &mut value.node else {
            panic!("query fixture");
        };
        let QueryBody::Select(select) = &mut query.body else {
            panic!("select fixture");
        };
        select.targets[0].expression = AstExpr::Array(vec![crate::ast::Expr::Literal(
            crate::ast::Literal::Integer(1),
        )]);
        let error = value.deparse_sql(Dialect::sqlite()).unwrap_err();
        assert!(error.is_invalid_syntax());
        assert_eq!(error.dialect(), Dialect::sqlite());
    }

    #[cfg(not(feature = "sqlite"))]
    #[test]
    fn unavailable_dialect_has_its_own_classification() {
        let mut value = statement("select 1", Dialect::postgres());
        value.dialect = Dialect::sqlite();
        let error = value.deparse_sql(Dialect::sqlite()).unwrap_err();
        assert!(error.is_unavailable_dialect());
        assert!(!error.is_dialect_mismatch());
    }

    #[rstest::rstest]
    #[case::cascade(Some(ForeignKeyAction::Cascade))]
    #[case::missing_action(None)]
    fn rejects_on_delete_column_lists_with_other_actions_as_invalid_syntax(
        #[case] action: Option<ForeignKeyAction>,
    ) {
        let mut statement = parse_statements(
            "CREATE TABLE child (parent_id integer, FOREIGN KEY (parent_id) REFERENCES parent(id) ON DELETE SET DEFAULT (parent_id))",
            Dialect::postgres(),
        )
        .next()
        .unwrap()
        .unwrap();
        let StatementNode::Table(table) = &mut statement.node else {
            panic!("table fixture");
        };
        let ColumnConstraint::ForeignKey(reference) = &mut table.constraints[0].constraint else {
            panic!("foreign key fixture");
        };
        reference.delete_action = action;

        let error = statement.deparse_sql(Dialect::postgres()).unwrap_err();
        assert!(error.is_invalid_syntax(), "{error}");
        assert_eq!(
            error.node(),
            "CREATE TABLE/ON DELETE column list",
            "{error}"
        );
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

impl HaveWithClause for Statement {
    fn with_clause(&self) -> Option<WithClause<'_>> {
        Self::with_clause(self)
    }
    fn with_clause_mut(&mut self) -> Option<WithClauseMut<'_>> {
        self.node.existing_with_editor(self.dialect)
    }
    fn ensure_with_clause(&mut self) -> Result<WithClauseMut<'_>, AstMutationError> {
        self.node.ensure_with_editor(self.dialect)
    }
}

impl WalkSqlMut for Statement {
    fn walk_sql_mut(&mut self, visitor: &mut impl AstVisitorMut) {
        self.node.walk_sql_mut(visitor);
    }
}

impl Statement {
    pub(crate) fn apply_source_edits(&mut self, edits: &mut impl ScopedSourceEdit) {
        self.node.apply_source_edits(edits);
    }
}

impl CommonTableExpression<'_> {
    /// Copy this declaration, preserving its body and all declaration attributes.
    ///
    /// Insertion rechecks dialect and placement constraints, including when the
    /// source was parsed without semantic analysis.
    pub fn to_owned(&self) -> CteDefinition {
        CteDefinition::copy_declaration(self.node, self.dialect)
    }
}
