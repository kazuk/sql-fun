#![doc = include_str!("sem.md")]

mod alter_owner;
mod alter_seq;
mod alter_table;
mod analysis_state;
mod base_context;
mod cast;
mod column_constraints;
mod comment;
mod conflict_masters;
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
mod frozen_base_context;
mod insert_statement;
mod names;
mod operators;
mod parameter_constraints;
pub(crate) mod parameter_resolution;
mod parse_context;
mod pg_type;
pub mod problem;
mod query;
mod scalar_expr;
mod schema_file_context;
mod search_path;
mod select_statement;
mod stack;
mod top_level_statement;
pub(crate) mod traversal;

#[cfg(test)]
pub(crate) use self::parse_context::conflict_info::infer_from_catalog;
mod type_system;
mod update_statement;
mod variable_set;
mod view_like;

pub(crate) use self::insert_statement::InsertDestination;

#[expect(deprecated)]
pub(crate) use self::conflict_masters::ConflictMasters;

pub use self::{
    alter_owner::AlterOwner,
    alter_seq::AlterSequence,
    alter_table::{AlterTable, AlterView},
    analysis_state::AnalysisState,
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
    comment::{Comment, ObjectKind},
    conflict_masters::{
        CatalogTypeIdentity, CollationDefinition, CollationIdentity, DatabaseCollation,
        OperatorClassDefinition, OperatorFamilyIdentity,
    },
    constraint::{AnalyzeConstraint, Constraint, PrimaryKeyConstraint},
    create_comp_type::CreateCompositeType,
    create_domain::CreateDomain,
    create_enum_type::CreateEnum,
    create_extension::CreateExtension,
    create_function::CreateFunction,
    create_index::{CreateIndex, IndexElement, IndexElementWithOrder, SortOrder},
    create_schema::CreateSchema,
    create_seq::CreateSequence,
    create_table::{
        ColumnCollection, ColumnDefinition, ColumnName, CreateTable, CreateTableMode, DropTable,
        TableName, ViewName,
    },
    create_table_as::CreateTableAs,
    create_view::CreateView,
    cte::{CteName, CteStatement, WithClause},
    data_source::{
        AliasSpec, DataSource, DataSourceRowFunctions, DataSourceRowsFromFunc, JoinKind, Nullable,
    },
    def_elem::{DefElemKey, DefElemList, analyze_def_elem_list},
    delete_statement::{DeleteStatement, analyze_delete_statement},
    error::AnalysisError,
    from_clause::FromClause,
    frozen_base_context::FrozenBaseContext,
    insert_statement::{
        InsertArbiter, InsertArbiterLookup, InsertConflict, InsertConstraintKind,
        InsertConstraintLookup, InsertInferenceModifiers, InsertSource, InsertStatement,
        InsertTarget, InsertValue, ViewInsertInfoRead, ViewInsertSupport, analyze_insert_statement,
    },
    names::{CatalogName, FullName, LocalName, OperatorName, SchemaName, ValidOperatorName},
    operators::{BinaryOperatorDefinition, OperatorDefinition, UnaryOperatorDefinition},
    parameter_constraints::{InputTypeConstraint, ParameterConstraint, ParameterConstraints},
    parse_context::{
        BaseParseContext, CastInfoRead, CatalogNameLookup, ConflictInfoRead,
        DefaultSourceAccessService, DiagnosticContext, FunctionInfoRead, FunctionInfoWrite,
        IndexInfoRead, OperatorInfoRead, ParseContext, SearchPathRead, SequenceInfoRead,
        SequenceInfoWrite, SourceAccessService, TableInfoRead, TableInfoWrite, TypeInfoRead,
        TypeInfoWrite, ViewInfoRead, ViewInfoWrite,
    },
    pg_type::{PgBuiltInType, TypeDefinition, TypeReference},
    problem::{AnalysisProblem, ParameterConstraintProblem, QueryProblem},
    query::{
        FunctionModifiers, QueryCte, QueryFrame, QueryFrameBound, QueryGrouping, QueryLock,
        QueryName, QueryNameResolution, QueryOrder, QueryWindow, ResolvedOperator, SelectClauses,
        SelectDestination, SemanticQuery, SemanticValues,
    },
    scalar_expr::{
        ArithExpr, ArrayExpr, BooleanExpr, CaseExpr, CoalesceExpr, CollateExpr,
        ColumnReferenceExpr, ConstrainedExpr, CteColumnRef, FuncCallExpr, FunctionBinding,
        ImplicitCastExpr, IndirectionExpr, MinMaxExpr, NamedArgExpr, NullTestExpr, OutputColumnRef,
        ParamRef, ScalarAnalysisContext, ScalarConstExpr, SemScalarExpr, SqlValueExpr, SubLinkExpr,
        SubQueryColumnRef, TableColumnRef, TypeCastExpr,
    },
    schema_file_context::SchemaFileContext,
    select_statement::{ResultColumnCollection, SelectStatement},
    type_system::{FunctionOverloadCollection, FunctionParam, OverloadVariant},
    update_statement::{UpdateStatement, analyze_update_statement},
    variable_set::{VariableSet, analyze_variable_set},
    view_like::ViewLike,
};
pub use crate::AstAndContextPair;

pub(crate) use self::{
    alter_table::AlterObjSubCommand,
    constraint::{ForeignKeyConstraint, UniqueConstraint},
    create_comp_type::{CompositeFieldCollection, CompositeFieldDefinition},
    create_index::IndexElementCollection,
    data_source::{AliasName, JoinUsingClause},
    parameter_constraints::{ConstraintGraph, SignatureAlternative},
    scalar_expr::{CaseExprAnalysisContext, CoalesceAnalysis},
    select_statement::SelectStatementNode,
    stack::with_stack,
};

use crate::StringSpan;
#[cfg(feature = "sqlite")]
use crate::{SqliteToken, syn::sqlite::Cmd};

use std::{
    fmt::Debug,
    path::{Path, PathBuf},
};

#[cfg(feature = "sqlite")]
use ::sql_fun_core::IVec;

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
    /// A statement whose semantic resolution failed recoverably.
    /// Diagnostics are retained by the context; no catalog definition is applied.
    UnresolvedStatement,

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
    pub(crate) const fn unresolved_statement() -> Self {
        Self::UnresolvedStatement
    }
    pub(crate) fn from_index_resolution(index: &Option<CreateIndex>) -> Self {
        if let Some(index) = index {
            Self::create_index(index)
        } else {
            Self::unresolved_statement()
        }
    }

    pub(crate) fn create_table(table: &CreateTable) -> Self {
        Self::CreateTable(table.clone())
    }
    pub(crate) fn create_function(function: &CreateFunction) -> Self {
        Self::CreateFunction(function.clone())
    }
    pub(crate) fn create_schema(schema: &CreateSchema) -> Self {
        Self::CreateSchema(schema.clone())
    }
    pub(crate) fn create_domain(domain: &CreateDomain) -> Self {
        Self::CreateDomain(domain.clone())
    }
    pub(crate) fn create_table_as(table: &CreateTableAs) -> Self {
        Self::CreateTableAs(table.clone())
    }
    pub(crate) fn create_index(index: &CreateIndex) -> Self {
        Self::CreateIndex(index.clone())
    }
    pub(crate) fn alter_table(table: &AlterTable) -> Self {
        Self::AlterTable(table.clone())
    }
    pub(crate) fn alter_view(view: &AlterView) -> Self {
        Self::AlterView(view.clone())
    }
    pub(crate) fn alter_sequence(sequence: &AlterSequence) -> Self {
        Self::AlterSequence(sequence.clone())
    }

    pub(crate) fn apply_relation_alter<TContext: ParseContext>(
        &self,
        context: TContext,
    ) -> Result<TContext, AnalysisError> {
        match self {
            Self::AlterTable(table) => context.apply_alter_table(table),
            Self::AlterView(view) => context.apply_alter_view(view),
            Self::AlterSequence(sequence) => context.apply_alter_sequence(sequence),
            _ => AnalysisError::raise_unexpected_input("alter relation result"),
        }
    }

    pub(crate) fn insert(insert: &InsertStatement) -> Self {
        Self::InsertStatement(insert.clone())
    }

    /// Inspect an analyzed INSERT.
    pub const fn as_insert(&self) -> Option<&InsertStatement> {
        match self {
            Self::InsertStatement(insert) => Some(insert),
            _ => None,
        }
    }

    pub(crate) fn select(query: &SelectStatement) -> Self {
        Self::SelectStatement(query.clone())
    }

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
        Self::semantic_analysis_sqlite_stmt(context, syn.into_statement(), tokens)
    }
}
