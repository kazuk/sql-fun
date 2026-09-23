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
}

impl CheckConstraint {
    /// create instance
    pub fn new(expr: Expr) -> Self {
        Self {
            expr: Box::new(expr),
        }
    }
}

use turso_parser::ast::RefAct as RefActIn;

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

use turso_parser::ast::RefArg as RefArgIn;

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

use turso_parser::ast::ForeignKeyClause as ForeignKeyClauseIn;

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

use turso_parser::ast::InitDeferredPred as InitDeferredPredIn;

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

use turso_parser::ast::DeferSubclause as DeferSubclauseIn;

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

use turso_parser::ast::TableConstraint as TableConstraintIn;

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
            TableConstraintIn::Check(expr) => Self::Check(CheckConstraint::new(Expr::from(*expr))),
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
}

impl CheckColumnConstraint {
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
    expr: Box<Expr>,
    typ: Option<GeneratedColumnType>,
}

impl GeneratedColumnConstraint {
    /// create instance
    pub fn new(expr: Expr, typ: Option<GeneratedColumnType>) -> Self {
        Self {
            expr: Box::new(expr),
            typ,
        }
    }
}

/// generated column type
#[derive(Debug)]
pub enum GeneratedColumnType {
    /// store value in database file
    Stored,
    /// compute value at record read
    Virtual,
}

use turso_parser::ast::GeneratedColumnType as GeneratedColumnTypeIn;
impl From<GeneratedColumnTypeIn> for GeneratedColumnType {
    fn from(value: GeneratedColumnTypeIn) -> Self {
        match value {
            GeneratedColumnTypeIn::Stored => Self::Stored,
            GeneratedColumnTypeIn::Virtual => Self::Virtual,
        }
    }
}

use turso_parser::ast::ColumnConstraint as ColumnConstraintIn;

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
            ColumnConstraintIn::Check(expr) => {
                Self::Check(CheckColumnConstraint::new(Expr::from(*expr)))
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
            ColumnConstraintIn::Generated { expr, typ } => {
                Self::Generated(GeneratedColumnConstraint::new(
                    Expr::from(*expr),
                    typ.map(GeneratedColumnType::from),
                ))
            }
        }
    }
}
