use crate::syn::sqlite::{Expr, IndexedColumn, Name, ResolveType, SortOrder, SortedColumn};

/// primary key constraint
#[derive(Debug, derive_getters::Getters)]
pub struct PrimaryKeyConstraint {
    columns: Vec<SortedColumn>,
    auto_increment: bool,
    conflict_clause: Option<ResolveType>,
}

impl PrimaryKeyConstraint {
    /// create instance
    pub const fn new(
        columns: Vec<SortedColumn>,
        auto_increment: bool,
        conflict_clause: Option<ResolveType>,
    ) -> Self {
        Self {
            columns,
            auto_increment,
            conflict_clause,
        }
    }
}

/// unique constraint
#[derive(Debug, derive_getters::Getters)]
pub struct UniqueConstraint {
    columns: Vec<SortedColumn>,
    conflict_clause: Option<ResolveType>,
}

impl UniqueConstraint {
    /// create instance
    pub const fn new(columns: Vec<SortedColumn>, conflict_clause: Option<ResolveType>) -> Self {
        Self {
            columns,
            conflict_clause,
        }
    }
}

/// check constraint
#[derive(Debug, derive_getters::Getters)]
pub struct CheckConstraint {
    expr: Box<Expr>,
    #[getter(skip)]
    source: Option<String>,
}

impl CheckConstraint {
    /// Create a CHECK payload, preserving parser-supplied source text.
    pub fn new(expr: &Expr, source: Option<&str>) -> Self {
        Self {
            expr: Box::new(expr.clone()),
            source: source.map(str::to_owned),
        }
    }

    /// Captured CHECK text, or None when the parser supplied no source.
    pub fn source(&self) -> Option<&str> {
        self.source.as_deref()
    }
}

use ::turso_parser::ast::RefAct as RefActIn;

/// ref action
#[derive(Debug)]
pub enum RefAct {
    /// set null
    SetNull,
    /// set default
    SetDefault,
    /// cascade
    Cascade,
    /// restrict
    Restrict,
    /// no action
    NoAction,
}

impl From<RefActIn> for RefAct {
    fn from(value: RefActIn) -> Self {
        match value {
            RefActIn::SetNull => Self::SetNull,
            RefActIn::SetDefault => Self::SetDefault,
            RefActIn::Cascade => Self::Cascade,
            RefActIn::Restrict => Self::Restrict,
            RefActIn::NoAction => Self::NoAction,
        }
    }
}

use ::turso_parser::ast::RefArg as RefArgIn;

/// ref argument
#[derive(Debug)]
pub enum RefArg {
    /// delete
    OnDelete(RefAct),
    /// insert
    OnInsert(RefAct),
    /// update
    OnUpdate(RefAct),
    /// match
    Match(Name),
}

impl From<RefArgIn> for RefArg {
    fn from(value: RefArgIn) -> Self {
        match value {
            RefArgIn::OnDelete(ref_act) => Self::OnDelete(RefAct::from(ref_act)),
            RefArgIn::OnInsert(ref_act) => Self::OnInsert(RefAct::from(ref_act)),
            RefArgIn::OnUpdate(ref_act) => Self::OnUpdate(RefAct::from(ref_act)),
            RefArgIn::Match(name) => Self::Match(Name::from(name)),
        }
    }
}

use ::turso_parser::ast::ForeignKeyClause as ForeignKeyClauseIn;

/// foreign key clause
#[derive(Debug, derive_getters::Getters)]
pub struct ForeignKeyClause {
    table_name: Name,
    columns: Vec<IndexedColumn>,
    args: Vec<RefArg>,
}

impl From<ForeignKeyClauseIn> for ForeignKeyClause {
    fn from(value: ForeignKeyClauseIn) -> Self {
        Self {
            table_name: Name::from(value.tbl_name),
            columns: value.columns.into_iter().map(IndexedColumn::from).collect(),
            args: value.args.into_iter().map(RefArg::from).collect(),
        }
    }
}

use ::turso_parser::ast::InitDeferredPred as InitDeferredPredIn;

/// init deferred predicate
#[derive(Debug, Default, Clone, Copy)]
pub enum InitDeferredPred {
    /// initially deferred
    InitiallyDeferred,
    /// initialiy immediate
    #[default]
    InitiallyImmediate,
}

impl InitDeferredPred {
    /// return true when InitiallyDeferred
    pub const fn init_deferred(&self) -> bool {
        matches!(self, Self::InitiallyDeferred)
    }
}

impl From<InitDeferredPredIn> for InitDeferredPred {
    fn from(value: InitDeferredPredIn) -> Self {
        match value {
            InitDeferredPredIn::InitiallyDeferred => Self::InitiallyDeferred,
            InitDeferredPredIn::InitiallyImmediate => Self::InitiallyImmediate,
        }
    }
}

use ::turso_parser::ast::DeferSubclause as DeferSubclauseIn;

/// defer subclause
#[derive(Debug, derive_getters::Getters)]
pub struct DeferSubclause {
    deferrable: bool,
    init_deferred: Option<InitDeferredPred>,
}

impl From<DeferSubclauseIn> for DeferSubclause {
    fn from(value: DeferSubclauseIn) -> Self {
        Self {
            deferrable: value.deferrable,
            init_deferred: value.init_deferred.map(InitDeferredPred::from),
        }
    }
}

/// foreign key constraint
#[derive(Debug, derive_getters::Getters)]
pub struct ForeignKeyConstraint {
    columns: Vec<IndexedColumn>,
    clause: ForeignKeyClause,
    defer_clause: Option<DeferSubclause>,
}

impl ForeignKeyConstraint {
    /// create instance
    pub const fn new(
        columns: Vec<IndexedColumn>,
        clause: ForeignKeyClause,
        defer_clause: Option<DeferSubclause>,
    ) -> Self {
        Self {
            columns,
            clause,
            defer_clause,
        }
    }
}

use ::turso_parser::ast::TableConstraint as TableConstraintIn;

/// table constraint
#[derive(Debug)]
pub enum TableConstraint {
    /// primary key
    PrimaryKey(PrimaryKeyConstraint),
    /// unique constraint
    Unique(UniqueConstraint),
    /// check constraint
    Check(CheckConstraint),
    /// foreign key constraint
    ForeignKey(ForeignKeyConstraint),
}

impl From<TableConstraintIn> for TableConstraint {
    fn from(value: TableConstraintIn) -> Self {
        match value {
            TableConstraintIn::PrimaryKey {
                columns,
                auto_increment,
                conflict_clause,
            } => Self::PrimaryKey(PrimaryKeyConstraint::new(
                columns.into_iter().map(SortedColumn::from).collect(),
                auto_increment,
                conflict_clause.map(ResolveType::from),
            )),
            TableConstraintIn::Unique {
                columns,
                conflict_clause,
            } => Self::Unique(UniqueConstraint::new(
                columns.into_iter().map(SortedColumn::from).collect(),
                conflict_clause.map(ResolveType::from),
            )),
            TableConstraintIn::Check { expr, source } => {
                Self::check(&Expr::from(*expr), source.as_deref())
            }
            TableConstraintIn::ForeignKey {
                columns,
                clause,
                defer_clause,
            } => Self::ForeignKey(ForeignKeyConstraint::new(
                columns.into_iter().map(IndexedColumn::from).collect(),
                ForeignKeyClause::from(clause),
                defer_clause.map(DeferSubclause::from),
            )),
        }
    }
}

/// primary key constraint
#[derive(Debug)]
pub struct PrimaryKeyColumnConstraint {
    order: Option<SortOrder>,
    conflict_clause: Option<ResolveType>,
    auto_increment: bool,
}

impl PrimaryKeyColumnConstraint {
    /// create instance
    pub const fn new(
        order: Option<SortOrder>,
        conflict_clause: Option<ResolveType>,
        auto_increment: bool,
    ) -> Self {
        Self {
            order,
            conflict_clause,
            auto_increment,
        }
    }

    /// get order
    pub const fn order(&self) -> Option<SortOrder> {
        self.order
    }

    /// get auto increment value
    pub const fn auto_increment(&self) -> bool {
        self.auto_increment
    }

    /// conflict clause
    pub const fn conflict_clause(&self) -> Option<&ResolveType> {
        self.conflict_clause.as_ref()
    }
}

/// not null column constraint
#[derive(Debug)]
pub struct NotNullColumnConstraint {
    nullable: bool,
    conflict_clause: Option<ResolveType>,
}

impl NotNullColumnConstraint {
    /// create instance
    pub const fn new(nullable: bool, conflict_clause: Option<ResolveType>) -> Self {
        Self {
            nullable,
            conflict_clause,
        }
    }

    /// nullable
    pub const fn nullable(&self) -> bool {
        self.nullable
    }

    /// conflict clause
    pub const fn conflict_clause(&self) -> Option<&ResolveType> {
        self.conflict_clause.as_ref()
    }
}

/// unique column constraint
#[derive(Debug)]
pub struct UniqueColumnConstraint {
    conflict_clause: Option<ResolveType>,
}

impl UniqueColumnConstraint {
    /// create instance
    pub const fn new(conflict_clause: Option<ResolveType>) -> Self {
        Self { conflict_clause }
    }

    /// conflict clause
    pub const fn conflict_clause(&self) -> Option<&ResolveType> {
        self.conflict_clause.as_ref()
    }
}

/// check column constraint
#[derive(Debug)]
pub struct CheckColumnConstraint {
    expr: Box<Expr>,
    source: Option<String>,
}

impl CheckColumnConstraint {
    /// Create a CHECK payload, preserving parser-supplied source text.
    pub fn new(expr: &Expr, source: Option<&str>) -> Self {
        Self {
            expr: Box::new(expr.clone()),
            source: source.map(str::to_owned),
        }
    }

    /// Captured CHECK text, or None when the parser supplied no source.
    pub fn source(&self) -> Option<&str> {
        self.source.as_deref()
    }

    /// expression
    pub fn expr(&self) -> &Expr {
        self.expr.as_ref()
    }
}

/// default column constraint
#[derive(Debug)]
pub struct DefaultColumnConstraint {
    expr: Box<Expr>,
}

impl DefaultColumnConstraint {
    /// create instance
    pub fn new(expr: Expr) -> Self {
        Self {
            expr: Box::new(expr),
        }
    }

    /// expression
    pub fn expr(&self) -> &Expr {
        self.expr.as_ref()
    }
}

/// collate column constraint
#[derive(Debug)]
pub struct CollateColumnConstraint {
    collation_name: Name,
}

impl CollateColumnConstraint {
    /// create instance
    pub const fn new(collation_name: Name) -> Self {
        Self { collation_name }
    }

    /// collation name
    pub const fn collation_name(&self) -> &Name {
        &self.collation_name
    }
}

/// foreign key column constraint
#[derive(Debug, derive_getters::Getters)]
pub struct ForeignKeyColumnConstraint {
    clause: ForeignKeyClause,
    defer_clause: Option<DeferSubclause>,
}

impl ForeignKeyColumnConstraint {
    /// create instance
    pub const fn new(clause: ForeignKeyClause, defer_clause: Option<DeferSubclause>) -> Self {
        Self {
            clause,
            defer_clause,
        }
    }
}

/// generated column constraint
#[derive(Debug, derive_getters::Getters)]
pub struct GeneratedColumnConstraint {
    #[getter(copy)]
    generated_always: bool,
    expr: Box<Expr>,
    #[getter(copy)]
    typ: Option<GeneratedColumnType>,
}

impl GeneratedColumnConstraint {
    /// create instance
    pub fn new(expr: &Expr, typ: &Option<GeneratedColumnType>, generated_always: bool) -> Self {
        Self {
            generated_always,
            expr: Box::new(expr.clone()),
            typ: *typ,
        }
    }
}

/// generated column type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GeneratedColumnType {
    /// store value in database file
    Stored,
    /// compute value at record read
    Virtual,
}

impl GeneratedColumnType {
    /// Construct explicit STORED syntax.
    pub const fn stored() -> Self {
        Self::Stored
    }
    /// Construct explicit VIRTUAL syntax.
    pub const fn virtual_column() -> Self {
        Self::Virtual
    }
}

use ::turso_parser::ast::GeneratedColumnType as GeneratedColumnTypeIn;
impl From<GeneratedColumnTypeIn> for GeneratedColumnType {
    fn from(value: GeneratedColumnTypeIn) -> Self {
        match value {
            GeneratedColumnTypeIn::Stored => Self::stored(),
            GeneratedColumnTypeIn::Virtual => Self::virtual_column(),
        }
    }
}

use ::turso_parser::ast::ColumnConstraint as ColumnConstraintIn;

/// column constraint
#[derive(Debug)]
pub enum ColumnConstraint {
    /// primary key
    PrimaryKey(PrimaryKeyColumnConstraint),
    /// not null
    NotNull(NotNullColumnConstraint),
    /// unique
    Unique(UniqueColumnConstraint),
    /// check
    Check(CheckColumnConstraint),
    /// default
    Default(DefaultColumnConstraint),
    /// collate
    Collate(CollateColumnConstraint),
    /// foreign key
    ForeignKey(ForeignKeyColumnConstraint),
    /// generated
    Generated(GeneratedColumnConstraint),
}

impl From<ColumnConstraintIn> for ColumnConstraint {
    fn from(value: ColumnConstraintIn) -> Self {
        match value {
            ColumnConstraintIn::PrimaryKey {
                order,
                conflict_clause,
                auto_increment,
            } => Self::PrimaryKey(PrimaryKeyColumnConstraint::new(
                order.map(SortOrder::from),
                conflict_clause.map(ResolveType::from),
                auto_increment,
            )),
            ColumnConstraintIn::NotNull {
                nullable,
                conflict_clause,
            } => Self::NotNull(NotNullColumnConstraint::new(
                nullable,
                conflict_clause.map(ResolveType::from),
            )),
            ColumnConstraintIn::Unique(resolve_type) => Self::Unique(UniqueColumnConstraint::new(
                resolve_type.map(ResolveType::from),
            )),
            ColumnConstraintIn::Check { expr, source } => {
                Self::check(&Expr::from(*expr), source.as_deref())
            }
            ColumnConstraintIn::Default(expr) => {
                Self::Default(DefaultColumnConstraint::new(Expr::from(*expr)))
            }
            ColumnConstraintIn::Collate { collation_name } => {
                Self::Collate(CollateColumnConstraint::new(Name::from(collation_name)))
            }
            ColumnConstraintIn::ForeignKey {
                clause,
                defer_clause,
            } => Self::ForeignKey(ForeignKeyColumnConstraint::new(
                ForeignKeyClause::from(clause),
                defer_clause.map(DeferSubclause::from),
            )),
            ColumnConstraintIn::Generated {
                expr,
                typ,
                generated_always,
            } => Self::generated(
                &Expr::from(*expr),
                &typ.map(GeneratedColumnType::from),
                generated_always,
            ),
        }
    }
}

impl ColumnConstraint {
    /// Construct a CHECK constraint from its expression and captured source.
    pub fn check(expr: &Expr, source: Option<&str>) -> Self {
        Self::Check(CheckColumnConstraint::new(expr, source))
    }

    /// Borrow the CHECK payload, if present.
    pub const fn as_check(&self) -> Option<&CheckColumnConstraint> {
        if let Self::Check(value) = self {
            Some(value)
        } else {
            None
        }
    }

    /// Construct a generated column without inferring omitted syntax.
    pub fn generated(
        expr: &Expr,
        typ: &Option<GeneratedColumnType>,
        generated_always: bool,
    ) -> Self {
        Self::Generated(GeneratedColumnConstraint::new(expr, typ, generated_always))
    }

    /// Borrow the generated-column payload, if present.
    pub const fn as_generated(&self) -> Option<&GeneratedColumnConstraint> {
        if let Self::Generated(value) = self {
            Some(value)
        } else {
            None
        }
    }
}

impl TableConstraint {
    /// Construct a CHECK constraint from its expression and captured source.
    pub fn check(expr: &Expr, source: Option<&str>) -> Self {
        Self::Check(CheckConstraint::new(expr, source))
    }

    /// Borrow the CHECK payload, if present.
    pub const fn as_check(&self) -> Option<&CheckConstraint> {
        if let Self::Check(value) = self {
            Some(value)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod test_constraint_conversion {
    use super::{ColumnConstraint, GeneratedColumnType, TableConstraint};
    use crate::test_helpers::SqliteSynBuilder;
    use ::turso_parser::ast::{
        ColumnConstraint as ColumnConstraintIn, Expr as ExprIn, Literal as LiteralIn,
        TableConstraint as TableConstraintIn,
    };

    #[test]
    fn preserves_generated_spelling_and_optional_storage_type() {
        for (spelling, always) in [("AS", false), ("GENERATED ALWAYS AS", true)] {
            for (suffix, storage) in [
                ("", None),
                (" STORED", Some(GeneratedColumnType::stored())),
                (" VIRTUAL", Some(GeneratedColumnType::virtual_column())),
            ] {
                let sql =
                    format!("CREATE TABLE items (base INT, derived INT {spelling} (42){suffix})");
                let stmt = SqliteSynBuilder.parse_as_stmt(&sql);
                let table = stmt.as_create_table().unwrap();
                let columns = table.body().as_columns_and_constraint().unwrap();
                let generated = columns.columns()[1].constraints()[0]
                    .constraint()
                    .as_generated()
                    .unwrap();
                assert_eq!(generated.generated_always(), always);
                assert_eq!(generated.typ(), storage);
                assert_eq!(generated.expr().numeric_literal_text(), Some("42"));
            }
        }
    }

    #[test]
    fn retains_column_and_table_check_source_verbatim() {
        let stmt = SqliteSynBuilder.parse_as_stmt(
            "CREATE TABLE items (value INT CONSTRAINT column_check CHECK ( value  >  0 ), CONSTRAINT table_check CHECK ( [value] > 1 /*keep*/ ))",
        );
        let table = stmt.as_create_table().unwrap();
        let columns = table.body().as_columns_and_constraint().unwrap();
        let named = &columns.columns()[0].constraints()[0];
        assert_eq!(named.name().unwrap().value(), "column_check");
        assert_eq!(
            named.constraint().as_check().unwrap().source(),
            Some("value  >  0")
        );
        let named = &columns.constraints()[0];
        assert_eq!(named.name().as_ref().unwrap().value(), "table_check");
        assert_eq!(
            named.constraint().as_check().unwrap().source(),
            Some("[value] > 1 /*keep*/")
        );
    }

    #[test]
    fn retains_check_source_absence_without_regenerating_it() {
        let column = ColumnConstraint::from(ColumnConstraintIn::Check {
            expr: Box::new(ExprIn::Literal(LiteralIn::Numeric("7".to_owned()))),
            source: None,
        });
        let column = column.as_check().unwrap();
        assert_eq!(column.source(), None);
        assert_eq!(column.expr().numeric_literal_text(), Some("7"));
        let table = TableConstraint::from(TableConstraintIn::Check {
            expr: Box::new(ExprIn::Literal(LiteralIn::Numeric("8".to_owned()))),
            source: None,
        });
        let table = table.as_check().unwrap();
        assert_eq!(table.source(), None);
        assert_eq!(table.expr().numeric_literal_text(), Some("8"));
    }
}
