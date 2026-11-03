//! Unresolved CREATE FOREIGN TABLE syntax, including unexpanded LIKE elements.
use crate::{
    ast::{
        AstMutationError, Column, Dialect, Name, TableConstraint, TableExtras, TryFromWithTokens,
        deparse::{DeparseError, SqlEmit, SqlWriter},
        postgres::required,
        traversal::{AstVisitorMut, WalkSqlMut},
    },
    context::ParseContext,
    engine_specific::{Postgres18Feature, Postgres18Syntax},
    sem::AnalysisError,
    syn::{self, ListOpt, Opt},
};
use ::sql_fun_core::IVec;

/// A foreign-data-wrapper option's identifier and string value.
#[derive(Clone, Debug)]
pub struct FdwOption {
    name: String,
    value: String,
}
impl FdwOption {
    /// Logical option identifier.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Option string value, without SQL quote delimiters.
    pub fn value(&self) -> &str {
        &self.value
    }
    fn collect(nodes: syn::NodeListOpt) -> Result<Vec<Self>, AnalysisError> {
        let mut options = Vec::new();
        for node in nodes.as_inner().unwrap_or_default() {
            options.push(Self::try_from(&required(
                node.as_def_elem().as_inner(),
                "FDW option",
            )?)?);
        }
        Ok(options)
    }
    pub(in crate::ast) fn emit_options(
        options: &[Self],
        w: &mut SqlWriter,
    ) -> Result<(), DeparseError> {
        if !options.is_empty() {
            w.sql(" OPTIONS (");
            w.list(options, ", ")?;
            w.sql(")");
        }
        Ok(())
    }
}
impl TryFrom<&syn::DefElem> for FdwOption {
    type Error = AnalysisError;
    fn try_from(value: &syn::DefElem) -> Result<Self, Self::Error> {
        Ok(Self {
            name: value.get_defname(),
            value: required(value.get_arg().as_string().get_sval(), "FDW option value")?,
        })
    }
}
impl SqlEmit for FdwOption {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        w.ident(&self.name)?;
        w.sql(" ");
        w.string(&self.value)
    }
}

/// An unexpanded LIKE clause; the source is an unresolved syntactic name.
#[derive(Clone, Debug)]
pub struct TableLike {
    source: Name,
    options: u32,
}
impl TableLike {
    /// Source relation's ordered identifier components.
    pub fn source(&self) -> &[String] {
        &self.source.0
    }
    /// Effective PostgreSQL LIKE option bit mask, as normalized by the parser.
    /// These are PostgreSQL CREATE_TABLE_LIKE_* bit values, not protobuf enum ordinals.
    pub const fn options(&self) -> u32 {
        self.options
    }
}
impl TryFrom<&syn::TableLikeClause> for TableLike {
    type Error = AnalysisError;
    fn try_from(value: &syn::TableLikeClause) -> Result<Self, Self::Error> {
        Ok(Self {
            source: Name::from(&required(value.get_relation().as_inner(), "LIKE source")?),
            options: value.get_options(),
        })
    }
}
impl SqlEmit for TableLike {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        w.sql("LIKE ");
        self.source.emit(w)?;
        // parsenodes.h TableLikeOption bit positions. ALL includes reserved bits,
        // so preserve its normalization using ALL plus exclusions where necessary.
        let names = [
            "COMMENTS",
            "COMPRESSION",
            "CONSTRAINTS",
            "DEFAULTS",
            "GENERATED",
            "IDENTITY",
            "INDEXES",
            "STATISTICS",
            "STORAGE",
        ];
        let all = self.options & !0x1ff != 0;
        if all {
            w.sql(" INCLUDING ALL");
        }
        for (index, name) in names.into_iter().enumerate() {
            let included = self.options & (1 << index) != 0;
            if included != all {
                w.sql(if included {
                    " INCLUDING "
                } else {
                    " EXCLUDING "
                });
                w.sql(name);
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
struct ForeignColumn {
    column: Column,
    options: Vec<FdwOption>,
}
impl TryFromWithTokens<&syn::ColumnDef> for ForeignColumn {
    type Error = AnalysisError;
    fn try_from_with_tokens(
        value: &syn::ColumnDef,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            column: Column::try_from_with_tokens(value, tokens)?,
            options: FdwOption::collect(value.get_fdwoptions())?,
        })
    }
}
impl SqlEmit for ForeignColumn {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        self.column.emit_with_fdw_options(w, &self.options)
    }
}

/// Syntactic kind of one foreign-table element.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ForeignTableElementKind {
    /// A column declaration.
    Column,
    /// A table constraint.
    Constraint,
    /// An unexpanded LIKE clause.
    Like,
}
impl ForeignTableElementKind {
    /// Column kind.
    pub const fn column() -> Self {
        Self::Column
    }
    /// Constraint kind.
    pub const fn constraint() -> Self {
        Self::Constraint
    }
    /// LIKE kind.
    pub const fn like() -> Self {
        Self::Like
    }
}
#[derive(Clone, Debug)]
enum ForeignElement {
    Column(ForeignColumn),
    Constraint(TableConstraint),
    Like(TableLike),
}
/// One ordered column, constraint or LIKE element in a foreign-table definition.
#[derive(Clone, Debug)]
pub struct ForeignTableElement(ForeignElement);
impl ForeignTableElement {
    /// Element kind, independent of the PostgreSQL parser tree.
    pub const fn kind(&self) -> ForeignTableElementKind {
        self.0.kind()
    }
    /// Column name for a column declaration.
    pub fn column_name(&self) -> Option<&str> {
        self.0.column_name()
    }
    /// FDW options for a column; empty for other elements.
    pub fn fdw_options(&self) -> &[FdwOption] {
        self.0.fdw_options()
    }
    /// LIKE syntax, without resolving its source or copying any columns.
    pub const fn like_clause(&self) -> Option<&TableLike> {
        self.0.like_clause()
    }
}
impl ForeignElement {
    const fn column(value: ForeignColumn) -> Self {
        Self::Column(value)
    }
    const fn constraint(value: TableConstraint) -> Self {
        Self::Constraint(value)
    }
    const fn like(value: TableLike) -> Self {
        Self::Like(value)
    }
    const fn kind(&self) -> ForeignTableElementKind {
        match self {
            Self::Column(_) => ForeignTableElementKind::column(),
            Self::Constraint(_) => ForeignTableElementKind::constraint(),
            Self::Like(_) => ForeignTableElementKind::like(),
        }
    }
    fn column_name(&self) -> Option<&str> {
        if let Self::Column(value) = self {
            Some(&value.column.name)
        } else {
            None
        }
    }
    fn fdw_options(&self) -> &[FdwOption] {
        if let Self::Column(value) = self {
            &value.options
        } else {
            &[]
        }
    }
    const fn like_clause(&self) -> Option<&TableLike> {
        if let Self::Like(value) = self {
            Some(value)
        } else {
            None
        }
    }
}
impl TryFromWithTokens<&syn::Node> for ForeignTableElement {
    type Error = AnalysisError;
    fn try_from_with_tokens(
        value: &syn::Node,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        Ok(Self(ForeignElement::try_from_with_tokens(value, tokens)?))
    }
}
impl TryFromWithTokens<&syn::Node> for ForeignElement {
    type Error = AnalysisError;
    fn try_from_with_tokens(
        value: &syn::Node,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        if let Some(value) = value.as_column_def().as_inner() {
            return Ok(Self::column(ForeignColumn::try_from_with_tokens(
                &value, tokens,
            )?));
        }
        if let Some(value) = value.as_constraint().as_inner() {
            return Ok(Self::constraint(TableConstraint::try_from_with_tokens(
                &value, tokens,
            )?));
        }
        if let Some(value) = value.as_table_like_clause().as_inner() {
            return Ok(Self::like(TableLike::try_from(&value)?));
        }
        AnalysisError::raise_unexpected_input("foreign table element")
    }
}
impl SqlEmit for ForeignTableElement {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        self.0.emit(w)
    }
}
impl SqlEmit for ForeignElement {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        match self {
            Self::Column(value) => value.emit(w),
            Self::Constraint(value) => value.emit(w),
            Self::Like(value) => value.emit(w),
        }
    }
}

/// CREATE FOREIGN TABLE syntax, with ordered unexpanded table elements.
#[derive(Clone, Debug)]
pub struct ForeignTable {
    name: Name,
    if_not_exists: bool,
    elements: Vec<ForeignTableElement>,
    extras: TableExtras,
    server: String,
    options: Vec<FdwOption>,
}
impl ForeignTable {
    /// Relation identifier components, without SQL quote delimiters.
    pub fn name(&self) -> &[String] {
        &self.name.0
    }
    /// Whether IF NOT EXISTS was specified.
    pub const fn if_not_exists(&self) -> bool {
        self.if_not_exists
    }
    /// Elements in source order, including unexpanded LIKE clauses.
    pub fn elements(&self) -> &[ForeignTableElement] {
        &self.elements
    }
    /// Foreign-server identifier spelling; no existence check is performed.
    pub fn server(&self) -> &str {
        &self.server
    }
    /// Ordered table-level FDW options.
    pub fn options(&self) -> &[FdwOption] {
        &self.options
    }
    /// Change the foreign-server name immediately, preserving all other syntax.
    /// Empty/NUL-containing identifiers are rejected without changing the owner.
    /// No name resolution is performed.
    pub fn set_server(&mut self, name: &str) -> Result<(), AstMutationError> {
        AstMutationError::check_identifier(name, Dialect::postgres())?;
        self.server = name.to_owned();
        Ok(())
    }
    pub(super) fn check_engine_features(
        &self,
        context: &mut impl ParseContext,
    ) -> Result<bool, AnalysisError> {
        if self
            .elements
            .iter()
            .any(|value| value.like_clause().is_some())
        {
            context.postgres18_syntax_used(Postgres18Syntax::ForeignTableLike)
        } else {
            Ok(true)
        }
    }
}
impl TryFromWithTokens<&syn::CreateForeignTableStmt> for ForeignTable {
    type Error = AnalysisError;
    fn try_from_with_tokens(
        value: &syn::CreateForeignTableStmt,
        tokens: &IVec<syn::ScanToken>,
    ) -> Result<Self, Self::Error> {
        let base = required(value.get_base_stmt().as_inner(), "foreign table definition")?;
        let name = Name::from(&required(
            base.get_relation().as_inner(),
            "foreign table name",
        )?);
        let mut elements = Vec::new();
        for node in base.get_table_elts().as_inner().unwrap_or_default() {
            elements.push(ForeignTableElement::try_from_with_tokens(&node, tokens)?);
        }
        Ok(Self {
            name,
            if_not_exists: base.get_if_not_exists(),
            elements,
            extras: TableExtras::try_from_with_tokens(&base, tokens)?,
            server: value.get_servername(),
            options: FdwOption::collect(value.get_options())?,
        })
    }
}
#[cfg(test)]
mod foreign_table_conversion_tests {
    use crate::{
        ast::{Dialect, ForeignTableElementKind},
        parse_statements,
        test_helpers::assert_postgres_structure,
    };

    #[rstest::rstest]
    #[case("CREATE FOREIGN TABLE f () SERVER missing")]
    #[case(r#"CREATE FOREIGN TABLE IF NOT EXISTS "App"."F" ("X" int OPTIONS (column_name 'remote'), y text COLLATE "C" DEFAULT 'a' NOT NULL, CONSTRAINT c CHECK ("X" > 0) NO INHERIT) INHERITS (p,q) SERVER "Remote" OPTIONS (schema_name 'r', table_name 'it''s')"#)]
    #[case(
        "CREATE FOREIGN TABLE f (LIKE a, x int, CHECK (x>0), LIKE b INCLUDING DEFAULTS INCLUDING CONSTRAINTS EXCLUDING DEFAULTS) SERVER s"
    )]
    #[case(
        "CREATE FOREIGN TABLE f (LIKE a INCLUDING ALL EXCLUDING IDENTITY EXCLUDING INDEXES) SERVER s"
    )]
    #[case(
        "CREATE FOREIGN TABLE f (LIKE a INCLUDING COMMENTS INCLUDING COMPRESSION INCLUDING GENERATED INCLUDING STATISTICS INCLUDING STORAGE) SERVER s"
    )]
    #[case("CREATE FOREIGN TABLE f (LIKE a INCLUDING IDENTITY INCLUDING INDEXES) SERVER s")]
    #[case(
        "CREATE FOREIGN TABLE f (x int GENERATED ALWAYS AS (1) VIRTUAL, y int GENERATED ALWAYS AS (2) STORED, NOT NULL x, CHECK (true) NOT ENFORCED) SERVER s"
    )]
    #[case(
        "CREATE FOREIGN TABLE f PARTITION OF p FOR VALUES FROM (MINVALUE) TO (10) SERVER s OPTIONS (a 'b')"
    )]
    #[case(
        "CREATE FOREIGN TABLE f PARTITION OF p (x WITH OPTIONS NOT NULL) FOR VALUES IN (1,2) SERVER s"
    )]
    #[case(
        "CREATE FOREIGN TABLE f PARTITION OF p FOR VALUES WITH (MODULUS 2, REMAINDER 0) SERVER s"
    )]
    #[case("CREATE FOREIGN TABLE f PARTITION OF p DEFAULT SERVER s")]
    fn native_tree_round_trip(#[case] sql: &str) {
        assert_postgres_structure(sql);
    }

    #[test]
    fn retains_ordered_elements_and_fdw_options_without_expansion() {
        let statement = parse_statements("CREATE FOREIGN TABLE f (LIKE missing INCLUDING DEFAULTS, x integer OPTIONS (remote 'x'), CHECK (x>0), LIKE other) SERVER absent OPTIONS (a 'b')", Dialect::postgres()).next().unwrap().unwrap();
        let value = statement.foreign_table().unwrap();
        assert_eq!(value.name(), ["f"]);
        assert_eq!(value.server(), "absent");
        assert_eq!(value.elements().len(), 4);
        assert_eq!(value.elements()[0].kind(), ForeignTableElementKind::like());
        assert_eq!(
            value.elements()[0].like_clause().unwrap().source(),
            ["missing"]
        );
        assert_eq!(value.elements()[0].like_clause().unwrap().options(), 1 << 3);
        assert_eq!(value.elements()[1].column_name(), Some("x"));
        assert_eq!(value.elements()[1].fdw_options()[0].name(), "remote");
        assert_eq!(value.elements()[1].fdw_options()[0].value(), "x");
        assert_eq!(
            value.elements()[2].kind(),
            ForeignTableElementKind::constraint()
        );
        assert_eq!(
            value.elements()[3].like_clause().unwrap().source(),
            ["other"]
        );
        assert_eq!(value.options()[0].value(), "b");
    }
}

impl SqlEmit for ForeignTable {
    fn emit(&self, w: &mut SqlWriter) -> Result<(), DeparseError> {
        w.postgres_only("CREATE FOREIGN TABLE")?;
        w.sql("CREATE FOREIGN TABLE ");
        if self.if_not_exists {
            w.sql("IF NOT EXISTS ");
        }
        self.name.emit(w)?;
        if self.extras.partition_bound.is_some() {
            let [parent] = self.extras.inherits.as_slice() else {
                return w.invalid(
                    "foreign table PARTITION OF",
                    "exactly one parent is required",
                );
            };
            w.sql(" PARTITION OF ");
            parent.emit(w)?;
        }
        if !self.elements.is_empty() || self.extras.partition_bound.is_none() {
            w.sql(" (");
            w.list(&self.elements, ", ")?;
            w.sql(")");
        }
        self.extras.emit(w)?;
        w.sql(" SERVER ");
        w.ident(&self.server)?;
        FdwOption::emit_options(&self.options, w)
    }
}

impl WalkSqlMut for ForeignTable {
    fn walk_sql_mut(&mut self, visitor: &mut impl AstVisitorMut) {
        self.elements.walk_sql_mut(visitor);
        self.extras.partition_bound.walk_sql_mut(visitor);
    }
}
impl WalkSqlMut for ForeignTableElement {
    fn walk_sql_mut(&mut self, visitor: &mut impl AstVisitorMut) {
        self.0.walk_sql_mut(visitor);
    }
}
impl WalkSqlMut for ForeignElement {
    fn walk_sql_mut(&mut self, visitor: &mut impl AstVisitorMut) {
        match self {
            Self::Column(value) => value.column.walk_sql_mut(visitor),
            Self::Constraint(value) => value.walk_sql_mut(visitor),
            Self::Like(_) => {}
        }
    }
}
