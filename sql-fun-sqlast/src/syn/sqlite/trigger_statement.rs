use crate::syn::sqlite::{
    Expr, FromClause, Name, QualifiedName, ResolveType, ResultColumn, Select, Set, Upsert,
};

use turso_parser::ast::TriggerTime as TriggerTimeIn;

/// trigger time option
#[derive(Debug)]
pub enum TriggerTime {
    /// before
    Before,
    /// after
    After,
    /// instead-of
    InsteadOf,
}

impl From<TriggerTimeIn> for TriggerTime {
    fn from(value: TriggerTimeIn) -> Self {
        match value {
            TriggerTimeIn::Before => Self::Before,
            TriggerTimeIn::After => Self::After,
            TriggerTimeIn::InsteadOf => Self::InsteadOf,
        }
    }
}

use turso_parser::ast::TriggerEvent as TriggerEventIn;

/// trigger event value
#[derive(Debug)]
pub enum TriggerEvent {
    /// delete
    Delete,
    /// insert
    Insert,
    /// update
    Update,
    /// update-of
    UpdateOf(Vec<Name>),
}

impl From<TriggerEventIn> for TriggerEvent {
    fn from(value: TriggerEventIn) -> Self {
        match value {
            TriggerEventIn::Delete => Self::Delete,
            TriggerEventIn::Insert => Self::Insert,
            TriggerEventIn::Update => Self::Update,
            TriggerEventIn::UpdateOf(names) => {
                Self::UpdateOf(names.into_iter().map(Name::from).collect())
            }
        }
    }
}

/// trigger command update option
#[derive(derive_getters::Getters, Debug)]
pub struct TriggerCmdUpdate {
    or_conflict: Option<ResolveType>,
    table_name: Name,
    sets: Vec<Set>,
    from: Option<FromClause>,
    where_clause: Option<Box<Expr>>,
}

impl TriggerCmdUpdate {
    /// create instance
    pub fn new(
        or_conflict: Option<ResolveType>,
        table_name: Name,
        sets: Vec<Set>,
        from: Option<FromClause>,
        where_clause: Option<Expr>,
    ) -> Self {
        Self {
            or_conflict,
            table_name,
            sets,
            from,
            where_clause: where_clause.map(Box::new),
        }
    }
}

/// trigger command insert option
#[derive(derive_getters::Getters, Debug)]
pub struct TriggerCmdInsert {
    or_conflict: Option<ResolveType>,
    table_name: Name,
    col_names: Vec<Name>,
    select: Select,
    upsert: Option<Box<Upsert>>,
    returning: Vec<ResultColumn>,
}

impl TriggerCmdInsert {
    /// create instance
    pub fn new(
        or_conflict: Option<ResolveType>,
        table_name: Name,
        col_names: Vec<Name>,
        select: Select,
        upsert: Option<Upsert>,
        returning: Vec<ResultColumn>,
    ) -> Self {
        Self {
            or_conflict,
            table_name,
            col_names,
            select,
            upsert: upsert.map(Box::new),
            returning,
        }
    }
}

/// trigger command delete option
#[derive(derive_getters::Getters, Debug)]
pub struct TriggerCmdDelete {
    table_name: Name,
    where_clause: Option<Box<Expr>>,
}

impl TriggerCmdDelete {
    /// create instance
    pub fn new(table_name: Name, where_clause: Option<Expr>) -> Self {
        Self {
            table_name,
            where_clause: where_clause.map(Box::new),
        }
    }
}

use turso_parser::ast::TriggerCmd as TriggerCmdIn;

/// trigger command
#[derive(Debug)]
pub enum TriggerCmd {
    /// update
    Update(TriggerCmdUpdate),
    /// insert
    Insert(TriggerCmdInsert),
    /// delete
    Delete(TriggerCmdDelete),
    /// select
    Select(Select),
}

impl From<TriggerCmdIn> for TriggerCmd {
    fn from(value: TriggerCmdIn) -> Self {
        match value {
            TriggerCmdIn::Update {
                or_conflict,
                tbl_name,
                sets,
                from,
                where_clause,
            } => Self::Update(TriggerCmdUpdate::new(
                or_conflict.map(ResolveType::from),
                Name::from(tbl_name),
                sets.into_iter().map(Set::from).collect(),
                from.map(FromClause::from),
                where_clause.map(|v| Expr::from(*v)),
            )),
            TriggerCmdIn::Insert {
                or_conflict,
                tbl_name,
                col_names,
                select,
                upsert,
                returning,
            } => Self::Insert(TriggerCmdInsert::new(
                or_conflict.map(ResolveType::from),
                Name::from(tbl_name),
                col_names.into_iter().map(Name::from).collect(),
                Select::from(select),
                upsert.map(|v| Upsert::from(*v)),
                returning.into_iter().map(ResultColumn::from).collect(),
            )),
            TriggerCmdIn::Delete {
                tbl_name,
                where_clause,
            } => Self::Delete(TriggerCmdDelete::new(
                Name::from(tbl_name),
                where_clause.map(|v| Expr::from(*v)),
            )),
            TriggerCmdIn::Select(select) => Self::Select(Select::from(select)),
        }
    }
}

/// create trigger statement
#[derive(derive_getters::Getters, Debug)]
pub struct CreateTriggerStatement {
    temporary: bool,
    if_not_exists: bool,
    trigger_name: QualifiedName,
    time: Option<TriggerTime>,
    event: TriggerEvent,
    table_name: QualifiedName,
    for_each_row: bool,
    when_clause: Option<Box<Expr>>,
    commands: Vec<TriggerCmd>,
}

impl CreateTriggerStatement {
    /// create instance
    #[expect(clippy::too_many_arguments)]
    pub fn new(
        temporary: bool,
        if_not_exists: bool,
        trigger_name: QualifiedName,
        time: Option<TriggerTime>,
        event: TriggerEvent,
        table_name: QualifiedName,
        for_each_row: bool,
        when_clause: Option<Expr>,
        commands: Vec<TriggerCmd>,
    ) -> Self {
        Self {
            temporary,
            if_not_exists,
            trigger_name,
            time,
            event,
            table_name,
            for_each_row,
            when_clause: when_clause.map(Box::new),
            commands,
        }
    }
}

/// drop trigger statement
#[derive(derive_getters::Getters, Debug)]
pub struct DropTriggerStatement {
    if_exists: bool,
    trigger_name: QualifiedName,
}

impl DropTriggerStatement {
    /// create instance
    pub fn new(if_exists: bool, trigger_name: QualifiedName) -> Self {
        Self {
            if_exists,
            trigger_name,
        }
    }
}
