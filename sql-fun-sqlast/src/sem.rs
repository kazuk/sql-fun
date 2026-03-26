#![doc = include_str!("sem.md")]

mod alter_owner;
mod alter_seq;
mod alter_table;
mod base_context;
mod cast;
mod column_constraints;
mod comment;
mod constraint;
mod create_comp_type;
mod create_domain;
mod create_enum_type;
mod create_extension;
mod create_function;
mod create_index;
mod create_schema;
mod create_seq;
mod create_table;
mod create_table_as;
mod create_view;
mod cte;
mod data_source;
mod def_elem;
mod delete_statement;
mod error;
mod from_clause;
mod insert_statement;
mod names;
mod operators;
mod parse_context;
mod pg_type;
mod scalar_expr;
mod schema_file_context;
mod search_path;
mod select_statement;
mod top_level_statement;
mod type_system;
mod update_statement;
mod variable_set;
mod view_like;

pub mod problem;

#[cfg(feature = "sqlite")]
use sql_fun_core::IVec;
#[cfg(feature = "sqlite")]
use turso_parser::ast::Cmd;

#[expect(deprecated)]
pub use self::{
    alter_owner::AlterOwner,
    alter_seq::AlterSequence,
    alter_table::{AlterTable, AlterView},
    base_context::BaseContext,
    cast::{CastContext, CastDefinition},
    column_constraints::{
        AttrDeferrabilityColumnConstraint, AttrDeferredColumnConstraint,
        AttrImmediateColumnConstraint, CheckColumnConstraint, CollateColumnConstraint,
        ColumnConstraint, ColumnConstraintKind, ColumnConstraintTrait, ColumnNullabilityConstraint,
        ConflictClause, DefaultColumnConstraint, Deferrability, Deferrable,
        ExclusionColumnConstraint, ForeignKeyAction, ForeignKeyColumnConstraint,
        ForeignKeyMatchType, GeneratedColumnConstraint, GeneratedColumnMode,
        IdentityColumnConstraint, IdentityColumnMode, PrimaryKeyColumnConstraint,
        UniqueColumnConstraint, Validity,
    },
    comment::{Comment, analyze_comment},
    constraint::{AnalyzeConstraint, Constraint, PrimaryKeyConstraint},
    create_comp_type::{CreateCompositeType, analyze_create_comp_type},
    create_domain::{CreateDomain, analyze_create_domain},
    create_enum_type::{CreateEnum, analyze_create_enum_type},
    create_extension::{CreateExtension, analyze_create_extension},
    create_function::{CreateFunction, analyze_create_function},
    create_index::{
        CreateIndex, IndexElement, IndexElementWithOrder, SortOrder, analyze_index_stmt,
    },
    create_schema::{CreateSchema, analyze_create_schema},
    create_seq::{CreateSequence, analyze_create_seq},
    create_table::{
        ColumnCollection, ColumnDefinition, ColumnName, CreateTable, CreateTableMode, DropTable,
        TableName, ViewName,
    },
    create_table_as::CreateTableAs,
    create_view::{CreateView, analyze_create_view},
    cte::{CteName, CteStatement, WithClause},
    data_source::{
        AliasSpec, DataSource, DataSourceRowFunctions, DataSourceRowsFromFunc, JoinKind, Nullable,
    },
    def_elem::{DefElemKey, DefElemList, analyze_def_elem_list},
    delete_statement::{DeleteStatement, analyze_delete_statement},
    error::AnalysisError,
    from_clause::FromClause,
    insert_statement::{InsertStatement, analyze_insert_statement},
    names::{CatalogName, FullName, LocalName, OperatorName, SchemaName, ValidOperatorName},
    operators::{BinaryOperatorDefinition, OperatorDefinition, UnaryOperatorDefinition},
    parse_context::{
        BaseParseContext, CastInfoRead, DefaultSourceAccessService, FunctionInfoRead,
        FunctionInfoWrite, IndexInfoRead, OperatorInfoRead, ParseContext, SearchPathRead,
        SequenceInfoRead, SequenceInfoWrite, SourceAccessService, TableInfoRead, TableInfoWrite,
        TypeInfoRead, TypeInfoWrite, ViewInfoRead, ViewInfoWrite,
    },
    pg_type::{PgBuiltInType, TypeDefinition, TypeReference},
    problem::AnalysisProblem,
    scalar_expr::{
        ArithExpr, ArrayExpr, BooleanExpr, CaseExpr, CoalesceExpr, CollateExpr,
        ColumnReferenceExpr, CteColumnRef, FuncCallExpr, ImplicitCastExpr, IndirectionExpr,
        MinMaxExpr, NamedArgExpr, NullTestExpr, ParamRef, ScalarAnalysisContext, ScalarConstExpr,
        SemScalarExpr, SqlValueExpr, SubLinkExpr, SubQueryColumnRef, TableColumnRef, TypeCastExpr,
        analyze_scaler_expr,
    },
    schema_file_context::SchemaFileContext,
    select_statement::{SelectStatement, analyze_select},
    top_level_statement::{analyze_node, semantic_analysis},
    type_system::{FunctionOverloadCollection, FunctionParam, OverloadVariant},
    update_statement::{UpdateStatement, analyze_update_statement},
    variable_set::{VariableSet, analyze_variable_set},
    view_like::ViewLike,
};

pub use crate::AstAndContextPair;
#[cfg(feature = "sqlite")]
use crate::{SqliteToken, syn::sqlite::Stmt};

use crate::StringSpan;

use std::{
    fmt::Debug,
    path::{Path, PathBuf},
};

/// source path and span pair
#[derive(Debug, Clone)]
pub struct SourcePathWithSpan {
    source_path: PathBuf,
    span: StringSpan,
}

impl SourcePathWithSpan {
    /// create instance
    pub fn new<P>(path: P, span: &StringSpan) -> Self
    where
        P: AsRef<Path>,
    {
        Self {
            source_path: path.as_ref().to_path_buf(),
            span: *span,
        }
    }

    /// get a source path
    pub fn source_path(&self) -> &Path {
        &self.source_path
    }

    /// get a source span
    pub const fn span(&self) -> &StringSpan {
        &self.span
    }
}

/// can get source path with span
pub trait GetSourcePathWithSpan {
    /// get source path with span
    fn get_source_path_with_span(&self) -> &SourcePathWithSpan;

    /// get source path
    fn get_source_path(&self) -> &Path {
        self.get_source_path_with_span().source_path()
    }

    /// get source span
    fn get_span(&self) -> &StringSpan {
        self.get_source_path_with_span().span()
    }
}

/// support mutate source path with span
pub trait MutSourcePathWithSpan {
    /// set source path with span
    fn set_source_path_with_span(&mut self, value: &SourcePathWithSpan);
}

/// cant get optional source path with span
pub trait GetSourcePathWithSpanOpt {
    /// get source path with span
    fn get_source_path_with_span(&self) -> Option<&SourcePathWithSpan>;

    /// get source path
    fn get_source_path(&self) -> Option<&Path> {
        self.get_source_path_with_span().map(|v| v.source_path())
    }

    /// get source span
    fn get_span(&self) -> Option<&StringSpan> {
        self.get_source_path_with_span().map(|v| v.span())
    }
}

/// constraint related schema change
#[derive(Debug, Clone)]
pub enum ImplicitChange {
    /// create an index
    CreateIndex(CreateIndex),
}

impl From<CreateIndex> for ImplicitChange {
    fn from(value: CreateIndex) -> Self {
        Self::CreateIndex(value)
    }
}

/// Semantic AST for top-level statement
#[derive(Debug, Clone)]
pub enum SemAst {
    /// `alter owner` statement
    AlterOwner(AlterOwner),

    /// `alter sequence` statement
    AlterSequence(AlterSequence),

    /// `alter table` statement
    AlterTable(AlterTable),

    /// `alter view` statement
    AlterView(AlterView),

    /// `comment` statement
    Comment(Comment),

    /// `create type` (composite type variant) statement
    CreateCompType(CreateCompositeType),

    /// `create type` (enum type variant) statement
    CreateEnum(CreateEnum),

    /// `create function` / `create procedure` statement
    CreateFunction(CreateFunction),

    /// `create domain` statement
    CreateDomain(CreateDomain),

    /// `create schema` statement
    CreateSchema(CreateSchema),

    /// `create sequence` statement
    CreateSequence(CreateSequence),

    /// `create table` statement
    CreateTable(CreateTable),

    /// `create materialized view` etc.
    CreateTableAs(CreateTableAs),

    /// `drop table` statement
    DropTable(DropTable),

    /// `create view` statement
    CreateView(CreateView),

    /// `create index` statement
    CreateIndex(CreateIndex),

    /// `select` statement
    SelectStatement(SelectStatement),

    /// `insert` statement
    InsertStatement(InsertStatement),

    /// `update` statement
    UpdateStatement(UpdateStatement),

    /// `delete` statement
    DeleteStatement(DeleteStatement),

    /// `create extension` statement
    CreateExtension(CreateExtension),

    /// variable set statement
    VariableSet(VariableSet),
}

impl SemAst {
    /// unwrap node as `SelectStatement`
    #[must_use]
    pub fn into_select_statemtnt(self) -> Option<SelectStatement> {
        match self {
            Self::SelectStatement(select_statement) => Some(select_statement),
            _ => None,
        }
    }

    /// unwrap node as [`DropTable`]
    #[must_use]
    pub fn into_drop_table(self) -> Option<DropTable> {
        if let Self::DropTable(t) = self {
            Some(t)
        } else {
            None
        }
    }

    /// cast as [`DropTable`]
    pub const fn as_drop_table(&self) -> Option<&DropTable> {
        if let Self::DropTable(t) = self {
            Some(t)
        } else {
            None
        }
    }

    /// cast as [`CreateTable`]
    pub const fn as_create_table(&self) -> Option<&CreateTable> {
        if let Self::CreateTable(t) = self {
            Some(t)
        } else {
            None
        }
    }

    /// analyze [`Cmd`]
    #[cfg(feature = "sqlite")]
    pub fn semantic_analysis_sqlite<TContext>(
        context: TContext,
        syn: Cmd,
        tokens: &IVec<SqliteToken<'_>>,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError>
    where
        TContext: ParseContext,
    {
        match syn {
            Cmd::Explain(stmt) => {
                Self::semantic_analysis_sqlite_stmt(context, Stmt::from(stmt), tokens)
            }
            Cmd::ExplainQueryPlan(stmt) => {
                Self::semantic_analysis_sqlite_stmt(context, Stmt::from(stmt), tokens)
            }
            Cmd::Stmt(stmt) => {
                Self::semantic_analysis_sqlite_stmt(context, Stmt::from(stmt), tokens)
            }
        }
    }

    /// analyze [`Stmt`]
    #[cfg(feature = "sqlite")]
    pub fn semantic_analysis_sqlite_stmt<TContext>(
        _context: TContext,
        syn: Stmt,
        _tokens: &IVec<SqliteToken<'_>>,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError>
    where
        TContext: ParseContext,
    {
        match syn {
            Stmt::AlterTable(_alter_table) => todo!(),
            Stmt::Analyze(_analyze_statement) => todo!(),
            Stmt::Attach(_attach_statement) => todo!(),
            Stmt::Detach(_detach_statement) => todo!(),
            Stmt::Vacuum(_vacuum_statement) => todo!(),
            Stmt::Begin(_begin_statement) => todo!(),
            Stmt::Commit(_commit_statement) => todo!(),
            Stmt::Release(_release_statement) => todo!(),
            Stmt::Rollback(_rollback_statement) => todo!(),
            Stmt::Savepoint(_savepoint_statement) => todo!(),
            Stmt::CreateIndex(_create_index_statement) => todo!(),
            Stmt::DropIndex(_drop_index_statement) => todo!(),
            Stmt::Reindex(_reindex_statement) => todo!(),
            Stmt::Optimize(_optimize_statement) => todo!(),
            Stmt::CreateTable(_create_table_statement) => todo!(),
            Stmt::CreateVirtualTable(_create_virtual_table) => todo!(),
            Stmt::DropTable(_drop_table_statement) => todo!(),
            Stmt::CreateTrigger(_create_trigger_statement) => todo!(),
            Stmt::DropTrigger(_drop_trigger_statement) => todo!(),
            Stmt::CreateView(_create_view) => todo!(),
            Stmt::CreateMaterializedView(_create_materialized_view) => todo!(),
            Stmt::DropView(_drop_view) => todo!(),
            Stmt::CreateType(_create_type_statement) => todo!(),
            Stmt::DropType(_drop_type_statement) => todo!(),
            Stmt::Select(_select) => todo!(),
            Stmt::Update(_update) => todo!(),
            Stmt::Delete(_delete_statement) => todo!(),
            Stmt::Insert(_insert_statement) => todo!(),
            Stmt::Pragma(_pragma_statement) => todo!(),
        }
    }
}
