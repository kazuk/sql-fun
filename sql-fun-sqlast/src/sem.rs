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
mod create_trigger;
mod create_view;
mod cte;
mod data_source;
mod def_elem;
mod delete_statement;
mod error;
mod from_clause;
mod frozen_base_context;
mod insert_statement;
mod merge_statement;
mod names;
mod object_info;
mod operators;
pub(crate) mod ownership;
mod parameter_constraints;
pub(crate) mod parameter_resolution;
mod parse_context;
mod pg_type;
pub mod problem;
mod query;
mod scalar_expr;
mod schema_file_context;
pub(crate) mod search_path;
mod select_statement;
mod stack;
mod top_level_statement;
pub(crate) mod traversal;

mod type_system;
mod update_statement;
mod variable_set;
mod view_like;
pub(crate) mod view_write;
pub use self::create_trigger::{
    CreateTrigger, TriggerBody, TriggerConstraint, TriggerDependencies, TriggerEvent,
    TriggerEventColumn, TriggerFunction, TriggerGranularity, TriggerIdentity, TriggerReferences,
    TriggerTiming, TriggerTransition,
};
pub use self::view_write::{ViewAssignmentColumn, ViewWriteChange, ViewWriteInfo};

pub(crate) use self::insert_statement::{
    InsertClauseSemantics, InsertClauseValues, InsertTargetCollection, InsertValueCollection,
};
#[cfg(test)]
pub(crate) use self::parse_context::conflict_info::infer_from_catalog;
pub(crate) use self::update_statement::UpdateAssignmentCollection;

pub(crate) use self::insert_statement::InsertDestination;
pub(crate) use self::merge_statement::MergeDestination;
pub(crate) use self::merge_statement::MergeWhenCollection;
pub use self::merge_statement::{
    MergeAction, MergeInsert, MergeMatchKind, MergeStatement, MergeUpdate, MergeWhen,
    ViewMergeInfoRead, ViewMergeSupport, ViewMergeTriggers, analyze_merge_statement,
};

pub(crate) use self::conflict_masters::ConflictMasters;

#[expect(deprecated)]
pub use self::{
    alter_owner::{AlterOwner, OwnershipArgumentFailure, OwnershipLookup},
    alter_seq::{AlterSequence, SequenceDependency},
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
        AliasSpec, DataSource, DataSourceRowFunctions, DataSourceRowsFromFunc, JoinKind,
        MergeRecordDataSource, MergeRecordRole, Nullable,
    },
    def_elem::{DefElemKey, DefElemList, analyze_def_elem_list},
    delete_statement::{
        DeleteCursor, DeleteRowColumn, DeleteRowVersion, DeleteStatement, ViewDeleteInfoRead,
        ViewDeleteSupport, analyze_delete_statement,
    },
    error::AnalysisError,
    from_clause::FromClause,
    frozen_base_context::FrozenBaseContext,
    insert_statement::{
        InsertArbiter, InsertArbiterLookup, InsertConflict, InsertConstraintKind,
        InsertConstraintLookup, InsertInferenceModifiers, InsertSource, InsertStatement,
        InsertTarget, InsertValue, ViewInsertInfoRead, ViewInsertSupport, ViewSourceArbiter,
        analyze_insert_statement,
    },
    names::{CatalogName, FullName, LocalName, OperatorName, SchemaName, ValidOperatorName},
    object_info::{ExtensionInfoRead, Kind, ObjectInfoRead},
    operators::{BinaryOperatorDefinition, OperatorDefinition, UnaryOperatorDefinition},
    ownership::{
        OwnedObjectDefinition, OwnerSpecification, OwnershipIdentity, OwnershipInfoRead, Role,
        RoleInfoRead,
    },
    parameter_constraints::{InputTypeConstraint, ParameterConstraint, ParameterConstraints},
    parse_context::{
        BaseParseContext, CastInfoRead, CatalogNameLookup, ConflictArbiterInference,
        ConflictInfoRead, CursorInfo, DefaultSourceAccessService, DiagnosticContext,
        FunctionInfoRead, FunctionInfoWrite, IndexInfoRead, OperatorInfoRead, ParseContext,
        SearchPathRead, SequenceInfoRead, SequenceInfoWrite, SourceAccessService, TableInfoRead,
        TableInfoWrite, TypeInfoRead, TypeInfoWrite, ViewInfoRead, ViewInfoWrite,
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
        ColumnReferenceExpr, ConstrainedExpr, CteColumnRef, ExistsExpr, FuncCallExpr,
        FunctionBinding, GroupingArgument, GroupingExpr, ImplicitCastExpr, IndirectionExpr,
        MergeRecordColumnRef, MinMaxExpr, NamedArgExpr, NullTestExpr, OutputColumnRef, ParamRef,
        ScalarAnalysisContext, ScalarConstExpr, SemScalarExpr, SqlValueExpr, SubLinkExpr,
        SubQueryColumnRef, TableColumnRef, TypeCastExpr,
    },
    schema_file_context::SchemaFileContext,
    select_statement::{ResultColumnCollection, SelectStatement},
    type_system::{FunctionOverloadCollection, FunctionParam, OverloadVariant},
    update_statement::{
        UpdateAssignment, UpdateCursor, UpdateQueryValue, UpdateStatement, UpdateValue,
        ViewUpdateInfoRead, ViewUpdateSupport, analyze_update_statement,
    },
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
    delete_statement::{DeleteDestination, DeletePredicate},
    parameter_constraints::{ConstraintGraph, SignatureAlternative},
    parse_context::CursorBindings,
    scalar_expr::{CaseExprAnalysisContext, CoalesceAnalysis, grouping::ReferenceCapture},
    select_statement::{ResultColumn, SelectStatementNode},
    stack::with_stack,
    update_statement::{UpdateDestination, UpdatePredicate},
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

impl ImplicitChange {
    pub(crate) fn into_index(self) -> CreateIndex {
        match self {
            Self::CreateIndex(index) => index,
        }
    }
}
impl From<CreateIndex> for ImplicitChange {
    fn from(value: CreateIndex) -> Self {
        Self::CreateIndex(value)
    }
}

/// Semantic AST for top-level statement
#[derive(Debug, Clone)]
pub enum SemAst {
    /// A resolved CREATE TRIGGER statement.
    CreateTrigger(Box<CreateTrigger>),
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

    /// A bound catalog change affecting view write capabilities.
    ViewWriteChange(ViewWriteChange),

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

    /// `merge` statement
    MergeStatement(MergeStatement),

    /// `create extension` statement
    CreateExtension(CreateExtension),

    /// variable set statement
    VariableSet(VariableSet),
}

impl SemAst {
    pub(crate) const fn select_statement(&self) -> Option<&SelectStatement> {
        if let Self::SelectStatement(query) = self { Some(query) } else { None }
    }
    pub(crate) fn walk_trigger_command<V: self::traversal::SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        match self {
            Self::SelectStatement(query) => query.walk(visitor),
            Self::InsertStatement(insert) => {
                visitor.relation_identity(insert.target().full_name());
                insert.walk(visitor)
            }
            Self::UpdateStatement(update) => {
                visitor.relation_identity(update.target().full_name());
                update.walk(visitor)
            }
            Self::DeleteStatement(delete) => {
                visitor.relation_identity(delete.target().full_name());
                delete.walk(visitor)
            }
            _ => unreachable!("only resolved SQLite trigger commands are traversed here"),
        }
    }

    /// Retain a resolved trigger statement.
    pub fn create_trigger(trigger: &CreateTrigger) -> Self {
        Self::CreateTrigger(Box::new(trigger.clone()))
    }
    /// Inspect a resolved trigger statement.
    pub fn trigger(&self) -> Option<&CreateTrigger> {
        if let Self::CreateTrigger(trigger) = self {
            Some(trigger)
        } else {
            None
        }
    }
    pub(crate) fn query_result_columns(&self) -> Option<&ResultColumnCollection> {
        if let Self::SelectStatement(query) = self {
            Some(query.result_columns())
        } else {
            None
        }
    }

    pub(crate) fn view_write_change(change: &ViewWriteChange) -> Self {
        Self::ViewWriteChange(change.clone())
    }
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
            Self::UnresolvedStatement => Ok(context),
            Self::AlterTable(table) => context.apply_alter_table(table),
            Self::AlterView(view) => context.apply_alter_view(view),
            Self::AlterSequence(sequence) => context.apply_alter_sequence(sequence),
            _ => AnalysisError::raise_unexpected_input("alter relation result"),
        }
    }

    pub(crate) fn merge(merge: &MergeStatement) -> Self {
        Self::MergeStatement(merge.clone())
    }
    /// Inspect an analyzed MERGE.
    pub const fn as_merge(&self) -> Option<&MergeStatement> {
        match self {
            Self::MergeStatement(merge) => Some(merge),
            _ => None,
        }
    }
    pub(crate) fn delete(delete: &DeleteStatement) -> Self {
        Self::DeleteStatement(delete.clone())
    }
    /// Inspect an analyzed DELETE.
    pub const fn as_delete(&self) -> Option<&DeleteStatement> {
        match self {
            Self::DeleteStatement(delete) => Some(delete),
            _ => None,
        }
    }
    pub(crate) fn update(update: &UpdateStatement) -> Self {
        Self::UpdateStatement(update.clone())
    }
    /// Inspect an analyzed UPDATE.
    pub const fn as_update(&self) -> Option<&UpdateStatement> {
        match self {
            Self::UpdateStatement(update) => Some(update),
            _ => None,
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
