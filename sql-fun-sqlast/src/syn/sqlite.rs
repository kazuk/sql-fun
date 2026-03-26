//! Owned SQLite syntax IR for this crate.
//!
//! This module primarily exists to reshape `turso_parser` AST nodes into forms
//! that are easier to match on and dispatch into analysis functions, and to
//! keep type ownership inside this crate so future traits and helper APIs are
//! not constrained by orphan rules.
//!
//! Together with `crate::sqlite`, it forms the interface boundary to
//! `turso_parser`, while semantic objects in `crate::sem` remain shared
//! between PostgreSQL and SQLite.
//!
//! As a secondary benefit, it also acts as a boundary layer so changes in
//! `turso_parser` do not leak directly into the semantic layer.

mod analyze_statement;
mod column_definition;
mod connection_context_statement;
mod constraints;
mod delete_statement;
mod expr;
mod frame_clause;
mod from_clause;
mod function_tail;
mod group_by;
mod index_statement;
mod insert_statement;
mod like_operator;
mod literal;
mod maintenance_statement;
mod names;
mod over;
mod pragma;
mod resolve_type;
mod select;
mod sorted_column;
mod subquery_type;
mod table_internal_id;
mod table_management_statement;
mod transaction_statement;
mod trigger_statement;
mod type_;
mod type_statements;
mod unary_operator;
mod update_statement;
mod upsert;
mod view_management_statements;
mod window;
mod with;

use crate::syn::sqlite::pragma::PragmaBody;

pub use self::{
    analyze_statement::AnalyzeStatement,
    column_definition::ColumnDefinition,
    connection_context_statement::{AttachStatement, DetachStatement},
    constraints::{
        CheckColumnConstraint, CheckConstraint, CollateColumnConstraint, ColumnConstraint,
        DefaultColumnConstraint, DeferSubclause, ForeignKeyClause, ForeignKeyColumnConstraint,
        ForeignKeyConstraint, GeneratedColumnConstraint, InitDeferredPred, NotNullColumnConstraint,
        PrimaryKeyColumnConstraint, PrimaryKeyConstraint, RefAct, RefArg, TableConstraint,
        UniqueColumnConstraint, UniqueConstraint,
    },
    delete_statement::DeleteStatement,
    expr::{
        CaseWhenThenPair, Expr, ExprBetween, ExprBinary, ExprCase, ExprCast, ExprCollate,
        ExprColumn, ExprDoublyQualified, ExprFunctionCall, ExprFunctionCallStar, ExprInList,
        ExprInSelect, ExprInTable, ExprIsNull, ExprLike, ExprNotNull, ExprRaise, ExprRowId,
        ExprUnary, Operator, Parenthesized, Qualified, ResultColumnExpr, SubqueryResult, Variable,
    },
    frame_clause::{FrameBound, FrameClause, FrameExclude, FrameMode},
    from_clause::{
        FromClause, Indexed, JoinConstraint, JoinOperator, JoinType, JoinedSelectTable,
        SelectTable, SelectTableSelect, SelectTableSub, SelectTableTable, SelectTableTableCell,
    },
    function_tail::FunctionTail,
    group_by::GroupBy,
    index_statement::{
        CreateIndexStatement, DropIndexStatement, OptimizeStatement, ReindexStatement,
    },
    insert_statement::{InsertBody, InsertBodySelect, InsertStatement},
    like_operator::LikeOperator,
    literal::{
        BlobLiteral, BooleanLiteral, KeywordLiteral, Literal, NullLiteral, NumericLiteral,
        StringLiteral, TimestampKind, TimestampLiteral,
    },
    maintenance_statement::VacuumStatement,
    names::{Name, QualifiedName},
    over::Over,
    pragma::PragmaStatement,
    resolve_type::ResolveType,
    select::{
        CompoundOperator, CompoundSelect, Distinctness, IndexedColumn, Limit, OneSelect,
        OneSelectItem, ResultColumn, Select, SelectBody,
    },
    sorted_column::{NullsOrder, SortOrder, SortedColumn},
    subquery_type::{SubqueryType, SubqueryTypeExists, SubqueryTypeIn, SubqueryTypeRowValue},
    table_internal_id::TableInternalId,
    table_management_statement::{
        AlterTable, AlterTableBody, ColumnsAndConstraint, CreateTableBody, CreateTableStatement,
        CreateVirtualTable, DropTableStatement, NamedColumnConstraint, NamedTableConstraint,
        TableOptions,
    },
    transaction_statement::{
        BeginStatement, CommitStatement, ReleaseStatement, RollbackStatement, SavepointStatement,
        TransactionType,
    },
    trigger_statement::{
        CreateTriggerStatement, DropTriggerStatement, TriggerCmd, TriggerCmdDelete,
        TriggerCmdInsert, TriggerCmdUpdate, TriggerEvent, TriggerTime,
    },
    type_::{Type, TypeSize},
    type_statements::{CreateTypeBody, CreateTypeStatement, DropTypeStatement},
    unary_operator::UnaryOperator,
    update_statement::{Set, Update},
    upsert::{Upsert, UpsertDo, UpsertDoSet, UpsertIndex},
    view_management_statements::{CreateMaterializedView, CreateView, DropView},
    window::{Window, WindowDef},
    with::{CommonTableExpr, Materialized, With},
};

use turso_parser::ast::As as AsIn;

/// as clause
pub enum As {
    /// as
    As(Name),
    /// elided
    Elided(Name),
}

impl From<AsIn> for As {
    fn from(value: AsIn) -> Self {
        match value {
            AsIn::As(name) => Self::As(Name::from(name)),
            AsIn::Elided(name) => Self::Elided(Name::from(name)),
        }
    }
}

use turso_parser::ast::Stmt as StmtIn;

/// statements
pub enum Stmt {
    /// alter table
    AlterTable(AlterTable),
    /// analyze
    Analyze(AnalyzeStatement),
    /// attach
    Attach(AttachStatement),
    /// detach
    Detach(DetachStatement),
    /// vacuum
    Vacuum(VacuumStatement),

    /// begin
    Begin(BeginStatement),
    /// commit
    Commit(CommitStatement),
    /// release
    Release(ReleaseStatement),
    /// rollback
    Rollback(RollbackStatement),
    /// savepoint
    Savepoint(SavepointStatement),

    /// create index
    CreateIndex(CreateIndexStatement),
    /// drop index
    DropIndex(DropIndexStatement),
    /// reindex
    Reindex(ReindexStatement),
    /// optimize
    Optimize(OptimizeStatement),

    /// create table
    CreateTable(CreateTableStatement),
    /// create virtual table
    CreateVirtualTable(CreateVirtualTable),
    /// drop table
    DropTable(DropTableStatement),

    /// create trigger
    CreateTrigger(CreateTriggerStatement),
    /// drop trigger
    DropTrigger(DropTriggerStatement),

    /// create view
    CreateView(CreateView),
    /// create materialized view
    CreateMaterializedView(CreateMaterializedView),
    /// drop view
    DropView(DropView),

    /// create type
    CreateType(CreateTypeStatement),
    /// drop type
    DropType(DropTypeStatement),

    /// select
    Select(Select),
    /// update
    Update(Update),
    /// delete
    Delete(DeleteStatement),
    /// insert
    Insert(InsertStatement),

    /// pragma
    Pragma(PragmaStatement),
}

impl From<StmtIn> for Stmt {
    fn from(value: StmtIn) -> Self {
        match value {
            StmtIn::AlterTable(alter_table) => Self::AlterTable(AlterTable::from(alter_table)),
            StmtIn::Analyze { name } => {
                Self::Analyze(AnalyzeStatement::new(name.map(QualifiedName::from)))
            }
            StmtIn::Attach { expr, db_name, key } => Self::Attach(AttachStatement::new(
                Expr::from(*expr),
                Expr::from(*db_name),
                key.map(|k| Expr::from(*k)),
            )),
            StmtIn::Begin { typ, name } => Self::Begin(BeginStatement::new(
                typ.map(TransactionType::from),
                name.map(Name::from),
            )),
            StmtIn::Commit { name } => Self::Commit(CommitStatement::new(name.map(Name::from))),
            StmtIn::CreateIndex {
                unique,
                if_not_exists,
                idx_name,
                tbl_name,
                using,
                columns,
                with_clause,
                where_clause,
            } => Self::CreateIndex(CreateIndexStatement::new(
                unique,
                if_not_exists,
                QualifiedName::from(idx_name),
                Name::from(tbl_name),
                using.map(Name::from),
                columns.into_iter().map(SortedColumn::from).collect(),
                with_clause
                    .into_iter()
                    .map(|(n, e)| (Name::from(n), Expr::from(*e)))
                    .collect(),
                where_clause.map(|v| Expr::from(*v)),
            )),
            StmtIn::CreateTable {
                temporary,
                if_not_exists,
                tbl_name,
                body,
            } => Self::CreateTable(CreateTableStatement::new(
                temporary,
                if_not_exists,
                QualifiedName::from(tbl_name),
                CreateTableBody::from(body),
            )),
            StmtIn::CreateTrigger {
                temporary,
                if_not_exists,
                trigger_name,
                time,
                event,
                tbl_name,
                for_each_row,
                when_clause,
                commands,
            } => Self::CreateTrigger(CreateTriggerStatement::new(
                temporary,
                if_not_exists,
                QualifiedName::from(trigger_name),
                time.map(TriggerTime::from),
                TriggerEvent::from(event),
                QualifiedName::from(tbl_name),
                for_each_row,
                when_clause.map(|v| Expr::from(*v)),
                commands.into_iter().map(TriggerCmd::from).collect(),
            )),
            StmtIn::CreateView {
                temporary,
                if_not_exists,
                view_name,
                columns,
                select,
            } => Self::CreateView(CreateView::new(
                temporary,
                if_not_exists,
                QualifiedName::from(view_name),
                columns.into_iter().map(IndexedColumn::from).collect(),
                Select::from(select),
            )),
            StmtIn::CreateMaterializedView {
                if_not_exists,
                view_name,
                columns,
                select,
            } => Self::CreateMaterializedView(CreateMaterializedView::new(
                if_not_exists,
                QualifiedName::from(view_name),
                columns.into_iter().map(IndexedColumn::from).collect(),
                Select::from(select),
            )),
            StmtIn::CreateVirtualTable(create_virtual_table) => {
                Self::CreateVirtualTable(CreateVirtualTable::from(create_virtual_table))
            }
            StmtIn::CreateType {
                if_not_exists,
                type_name,
                body,
            } => Self::CreateType(CreateTypeStatement::new(
                if_not_exists,
                type_name,
                CreateTypeBody::from(body),
            )),
            StmtIn::Delete {
                with,
                tbl_name,
                indexed,
                where_clause,
                returning,
                order_by,
                limit,
            } => Self::Delete(DeleteStatement::new(
                with.map(With::from),
                QualifiedName::from(tbl_name),
                indexed.map(Indexed::from),
                where_clause.map(|v| Box::new(Expr::from(*v))),
                returning.into_iter().map(ResultColumn::from).collect(),
                order_by.into_iter().map(SortedColumn::from).collect(),
                limit.map(Limit::from),
            )),
            StmtIn::Detach { name } => Self::Detach(DetachStatement::new(Expr::from(*name))),
            StmtIn::DropIndex {
                if_exists,
                idx_name,
            } => Self::DropIndex(DropIndexStatement::new(
                if_exists,
                QualifiedName::from(idx_name),
            )),
            StmtIn::DropTable {
                if_exists,
                tbl_name,
            } => Self::DropTable(DropTableStatement::new(
                if_exists,
                QualifiedName::from(tbl_name),
            )),
            StmtIn::DropTrigger {
                if_exists,
                trigger_name,
            } => Self::DropTrigger(DropTriggerStatement::new(
                if_exists,
                QualifiedName::from(trigger_name),
            )),
            StmtIn::DropView {
                if_exists,
                view_name,
            } => Self::DropView(DropView::new(if_exists, QualifiedName::from(view_name))),
            StmtIn::DropType {
                if_exists,
                type_name,
            } => Self::DropType(DropTypeStatement::new(if_exists, type_name)),
            StmtIn::Insert {
                with,
                or_conflict,
                tbl_name,
                columns,
                body,
                returning,
            } => Self::Insert(InsertStatement::new(
                with.map(With::from),
                or_conflict.map(ResolveType::from),
                QualifiedName::from(tbl_name),
                columns.into_iter().map(Name::from).collect(),
                InsertBody::from(body),
                returning.into_iter().map(ResultColumn::from).collect(),
            )),
            StmtIn::Pragma { name, body } => Self::Pragma(PragmaStatement::new(
                QualifiedName::from(name),
                body.map(PragmaBody::from),
            )),
            StmtIn::Reindex { name } => {
                Self::Reindex(ReindexStatement::new(name.map(QualifiedName::from)))
            }
            StmtIn::Release { name } => Self::Release(ReleaseStatement::new(Name::from(name))),
            StmtIn::Rollback {
                tx_name,
                savepoint_name,
            } => Self::Rollback(RollbackStatement::new(
                tx_name.map(Name::from),
                savepoint_name.map(Name::from),
            )),
            StmtIn::Savepoint { name } => {
                Self::Savepoint(SavepointStatement::new(Name::from(name)))
            }
            StmtIn::Select(select) => Self::Select(Select::from(select)),
            StmtIn::Update(update) => Self::Update(Update::from(update)),
            StmtIn::Vacuum { name, into } => Self::Vacuum(VacuumStatement::new(
                name.map(Name::from),
                into.map(|e| Expr::from(*e)),
            )),
            StmtIn::Optimize { idx_name } => {
                Self::Optimize(OptimizeStatement::new(idx_name.map(QualifiedName::from)))
            }
        }
    }
}
