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
mod domain;
mod expr;
mod frame_clause;
mod from_clause;
mod function_tail;
mod group_by;
mod index_statement;
mod insert_statement;
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
mod update_statement;
mod upsert;
mod view_management_statements;
mod window;
mod with;

use crate::syn::sqlite::{domain::DomainConstraint, pragma::PragmaBody};

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
    domain::{CreateDomainStatement, DropDomainStatement},
    expr::{
        Array, CaseWhenThenPair, Expr, ExprBetween, ExprBinary, ExprCase, ExprCast, ExprCollate,
        ExprColumn, ExprDoublyQualified, ExprFunctionCall, ExprFunctionCallStar, ExprInList,
        ExprInSelect, ExprInTable, ExprIsNull, ExprLike, ExprNotNull, ExprRaise, ExprRowId,
        ExprUnary, FieldAccess, FieldAccessResolution, LikeOperator, Operator, Parenthesized,
        Qualified, SubqueryResult, Subscript, UnaryOperator, Variable,
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
        OneSelectItem, ResultColumn, ResultColumnExpr, Select, SelectBody,
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
    update_statement::{Set, Update},
    upsert::{Upsert, UpsertDo, UpsertDoSet, UpsertIndex},
    view_management_statements::{CreateMaterializedView, CreateView, DropView},
    window::{Window, WindowDef},
    with::{CommonTableExpr, Materialized, With},
};

use turso_parser::ast::As as AsIn;

/// as clause
#[derive(Clone, Debug)]
pub enum As {
    /// as
    As(Name),
    /// elided
    Elided(Name),
    /// implicit column name
    ImplicitColumnName(Name),
}

impl From<AsIn> for As {
    fn from(value: AsIn) -> Self {
        match value {
            AsIn::As(name) => Self::As(Name::from(name)),
            AsIn::Elided(name) => Self::Elided(Name::from(name)),
            AsIn::ImplicitColumnName(name) => Self::ImplicitColumnName(Name::from(name)),
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

    /// create domain
    CreateDomain(CreateDomainStatement),
    /// drop domain
    DropDomain(DropDomainStatement),

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
            StmtIn::CreateDomain {
                if_not_exists,
                domain_name,
                base_type,
                default,
                not_null,
                constraints,
            } => Self::CreateDomain(CreateDomainStatement::new(
                if_not_exists,
                &domain_name,
                &base_type,
                default.map(|v| Expr::from(*v)),
                not_null,
                constraints
                    .into_iter()
                    .map(DomainConstraint::from)
                    .collect(),
            )),
            StmtIn::DropDomain {
                if_exists,
                domain_name,
            } => Self::DropDomain(DropDomainStatement::new(if_exists, &domain_name)),
        }
    }
}

#[cfg(test)]
mod test_stmt_from {
    use testresult::TestResult;
    use turso_parser::{ast::Cmd, parser::Parser};

    #[rstest::rstest]
    #[case::alter_table("alter table users add column age int")]
    #[case::analyze("analyze table_one")]
    #[case::attach("attach database 'test.db' as test_db")]
    #[case::begin("begin")]
    #[case::commit("commit")]
    #[case::create_index("create index index_one on table_one (col1, col2)")]
    #[case::create_table("create table users (id int not null, name text)")]
    #[case::create_trigger(
        r#"CREATE TRIGGER update_customer_address UPDATE OF address ON customers
      BEGIN
        UPDATE orders SET address = new.address WHERE customer_name = old.name;
      END;"#
    )] // https://sqlite.org/lang_createtrigger.html
    #[case::create_view(r#"create view view_one as select id,name from table_one"#)]
    #[case::create_mat_view(
        r#"CREATE MATERIALIZED VIEW customer_totals AS
    SELECT
        customer_id,
        COUNT(*) AS order_count,
        SUM(amount) AS total_spent
    FROM orders
    GROUP BY customer_id"#
    )]
    #[case::create_virt_table("create virtual table vt using module_one")]
    #[case::create_type("CREATE TYPE platform_id AS UNION(telegram INT, slack TEXT, signal TEXT)")] // https://turso.tech/blog/turso-0.6.0
    #[case::delete("delete from users where id=1")]
    #[case::detach("detach database temp")]
    #[case::drop_index("drop index index_one")]
    #[case::drop_table("drop table table_one")]
    #[case::drop_trigger("drop trigger trigger_one")]
    #[case::drop_view("drop view view_one")]
    #[case::drop_type("drop type custom_type_one")]
    #[case::insert("insert into users (id, name) values(0, 'test user')")]
    #[case::pragma("PRAGMA index_info('idx52')")] // https://sqlite.org/pragma.html#syntax
    #[case::reindex("reindex index_one")]
    #[case::release("release savepoint savepoint_one")]
    #[case::rollback("rollback")]
    #[case::savepoint("savepoint savepoint_one")]
    #[case::select("select * from users")]
    #[case::update("update users set name='test user' where id=0 ")]
    #[case::vacuum("vacuum edge_db into 'edge.db'")]
    #[case::optimize("OPTIMIZE INDEX idx_articles")] // https://github.com/tursodatabase/turso/blob/main/docs/manual.md#index-maintenance
    #[case::create_domain("create domain user_name as text")]
    #[case::drop_domain("drop domain user_name")]
    fn test_stmt_from_simple(#[case] sql: &str) -> TestResult {
        let mut parser = Parser::new(sql.as_bytes());
        let cmd = parser.next_cmd()?.unwrap();
        let Cmd::Stmt(stmt) = cmd else {
            panic!("cmd is not stmt")
        };
        let _stmt = super::Stmt::from(stmt);
        Ok(())
    }
}
