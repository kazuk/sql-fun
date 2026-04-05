#![doc = include_str!("sem.md")]

mod alter_owner;
mod alter_seq;
mod alter_table;
mod base_context;
mod cast;
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

pub use self::{
    alter_owner::{AlterOwner, analyze_alter_owner},
    alter_seq::{AlterSequence, analyze_alter_seq},
    alter_table::{AlterTable, AlterView, analyze_alter_statement},
    base_context::BaseContext,
    cast::{CastContext, CastDefinition},
    comment::{Comment, analyze_comment},
    constraint::{AnalizeConstraint, Constraint, PrimaryKeyConstraint},
    create_comp_type::{CreateCompositType, analyze_create_comp_type},
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
        ColumnCollection, ColumnDefinition, ColumnName, CreateTable, TableName, ViewName,
        analyze_create_stmt,
    },
    create_table_as::{CreateTableAs, analyze_create_table_as},
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
        MinMaxExpr, NamedArgExpr, NullTestExpr, ParamRef, ScalarConstExpr, SemScalarExpr,
        SqlValueExpr, SubLinkExpr, SubQueryColumnRef, TableColumnRef, TypeCastExpr,
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

use std::fmt::Debug;

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
    CreateCompType(CreateCompositType),

    /// `create type` (enum type variant) statement
    CreateEnum(CreateEnum),

    /// `create function` / `create procedure` statement
    CreateFunction(CreateFunction),

    /// `create domain` statement
    CreateDomain(CreateDomain),

    /// `create schema` statement
    CreateSchema(CreateSchema),

    /// `create sequence` statemnt
    CreateSequence(CreateSequence),

    /// `create table` statement
    CreateTable(CreateTable),

    /// `create materialized view` etc.
    CreateTableAs(CreateTableAs),

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
}
