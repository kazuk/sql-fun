#![doc = include_str!("sem.md")]

mod alter_column;
mod alter_index;
mod alter_owner;
mod alter_seq;
mod alter_table;
mod column_rename;
mod source_flag;
pub use self::column_rename::ColumnRename;
mod analysis_state;
mod cast;
mod column_constraints;
mod comment;
pub(crate) mod conflict_info;
pub(crate) mod conflict_masters;
mod constraint;
mod create_comp_type;
mod create_domain;
mod create_enum_type;
mod create_extension;
mod create_function;
mod create_index;
mod create_rule;
mod create_schema;
mod create_seq;
mod create_table;
mod create_table_as;
mod create_trigger;
mod create_view;
mod cte;
mod cursor_info;
mod data_source;
mod declaration_collation;
mod def_elem;
mod delete_statement;
mod drop_dependency_behavior;
mod drop_rule;
mod drop_trigger;
mod error;
mod foreign_key_target;
mod from_clause;
mod insert_statement;
mod merge_statement;
mod names;
mod object_info;
mod operators;
mod overload_rank;
pub(crate) mod ownership;
mod parameter_constraints;
pub(crate) mod parameter_resolution;
pub(crate) mod pg_type;
pub mod problem;
mod query;
mod query_metadata;
mod resolution;
mod sampling_method;
mod scalar_expr;
pub(crate) mod search_path;
mod select_statement;
mod top_level_statement;
pub(crate) mod traversal;
mod wildcard_expansion;
pub use self::declaration_collation::DeclarationCollation;
pub use self::query_metadata::QueryMetadata;
pub use self::with_typmod::WithTypmod;

pub(crate) mod type_system;
mod update_statement;
mod variable_set;
mod view_definition;
mod view_like;
pub(crate) mod view_write;
mod with_typmod;
pub use self::create_rule::{
    CreateRule, RuleDependencies, RuleEvent, RuleIdentity, RuleReferences,
};
pub use self::create_trigger::{
    CreateTrigger, TriggerBody, TriggerConstraint, TriggerDependencies, TriggerEvent,
    TriggerEventColumn, TriggerFunction, TriggerGranularity, TriggerIdentity, TriggerReferences,
    TriggerTiming, TriggerTransition,
};
pub(crate) use self::data_source::{ProjectionColumn, WildcardProjection};
pub use self::drop_rule::DropRule;
pub(crate) use self::foreign_key_target::ForeignKeyTarget;
pub(crate) use self::overload_rank::OverloadRank;
pub use self::view_definition::{ViewCheckOption, ViewColumnLineage, ViewOption};
pub use self::view_write::{ViewAssignmentColumn, ViewWriteEvaluation, ViewWriteSupport};
pub(crate) use self::wildcard_expansion::{
    WildcardQualifier, WildcardQualifiers, WildcardRowsFunction, WildcardRowsProjection,
    WildcardRowsSource, WildcardRowsSources, WildcardSourceProjection, WildcardTypedRecord,
};

#[cfg(test)]
pub(crate) use self::conflict_info::infer_from_catalog;
pub use self::cursor_info::CursorInfo;
pub(crate) use self::insert_statement::{
    InsertClauseSemantics, InsertClauseValues, InsertTargetCollection, InsertValueCollection,
};
pub(crate) use self::resolution::ResolutionFailure;
pub(crate) use self::update_statement::UpdateAssignmentCollection;

pub(crate) use self::insert_statement::InsertDestination;
pub(crate) use self::merge_statement::MergeDestination;
pub(crate) use self::merge_statement::MergeWhenCollection;
pub use self::merge_statement::{
    MergeAction, MergeInsert, MergeMatchKind, MergeStatement, MergeUpdate, MergeWhen,
    ViewMergeSupport, ViewMergeTriggers, analyze_merge_statement,
};

pub use self::drop_dependency_behavior::DropDependencyBehavior;
pub use self::drop_trigger::DropTrigger;

#[expect(deprecated)]
pub use self::{
    alter_column::{
        AlterColumnAttributes, ColumnAttributeAction, ColumnAttributeBinding,
        ColumnAttributeOption, ColumnStatisticsTarget,
    },
    alter_index::{
        AlterIndex, IndexAlteration, IndexStatisticsChange, IndexStorageChange,
        IndexStorageParameter, IndexTarget,
    },
    alter_owner::{AlterOwner, OwnershipArgumentFailure, OwnershipLookup},
    alter_seq::{AlterSequence, SequenceDependency},
    alter_table::{AlterTable, AlterView},
    analysis_state::AnalysisState,
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
    conflict_info::{CatalogNameLookup, ConflictArbiterInference},
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
    create_index::{
        CreateIndex, IndexElement, IndexElementWithOrder, IndexStorageValue, SortOrder,
    },
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
        AliasSpec, DataSource, DataSourceRowFunctions, DataSourceRowsFromFunc,
        DeferredTableFunction, JoinKind, MergeRecordDataSource, MergeRecordRole, Nullable,
        SamplingInput, TableSampleSource,
    },
    def_elem::{DefElemKey, DefElemList, analyze_def_elem_list},
    delete_statement::{
        DeleteCursor, DeleteRowColumn, DeleteRowVersion, DeleteStatement, ViewDeleteSupport,
        analyze_delete_statement,
    },
    error::AnalysisError,
    from_clause::FromClause,
    insert_statement::{
        InsertArbiter, InsertArbiterLookup, InsertConflict, InsertConstraintKind,
        InsertConstraintLookup, InsertInferenceModifiers, InsertSource, InsertStatement,
        InsertTarget, InsertValue, ViewInsertSupport, ViewSourceArbiter, analyze_insert_statement,
    },
    names::{
        CatalogName, FullName, LocalName, OperatorName, SchemaName, ValidOperatorName,
        parse_client_type,
    },
    object_info::Kind,
    operators::{BinaryOperatorDefinition, OperatorDefinition, UnaryOperatorDefinition},
    ownership::{OwnedObjectDefinition, OwnerSpecification, OwnershipIdentity, Role},
    parameter_constraints::{InputTypeConstraint, ParameterConstraint, ParameterConstraints},
    pg_type::{
        ArrayTypeDefinition, BuiltinTypeDefinition, PgBuiltInType, TypeCategory, TypeDefinition,
        TypeReference,
    },
    problem::{AnalysisProblem, ParameterConstraintProblem, QueryProblem, TableFunctionProblem},
    query::{
        FunctionModifiers, QueryCte, QueryFrame, QueryFrameBound, QueryGrouping, QueryLock,
        QueryName, QueryNameResolution, QueryOrder, QueryWindow, ResolvedOperator, SelectClauses,
        SelectDestination, SemanticQuery, SemanticValues,
    },
    sampling_method::{SamplingMethod, SamplingMethodLookup},
    scalar_expr::{
        ArithExpr, ArrayExpr, ArraySubqueryExpr, ArraySubscript, ArraySubscripts, BooleanExpr,
        CaseExpr, CaseWhen, CoalesceExpr, CollateExpr, ColumnReferenceExpr, ConstrainedExpr,
        CteColumnRef, DistinctExpr, ExistsExpr, FuncCallArgs, FuncCallExpr, FunctionBinding,
        GroupingArgument, GroupingExpr, ImplicitCastExpr, IndirectionExpr, MembershipComparison,
        MembershipExpr, MergeRecordColumnRef, MinMaxExpr, NamedArgExpr, NullTestExpr,
        OutputColumnRef, ParamRef, QualifiedColumnFailure, ScalarAnalysisContext, ScalarConstExpr,
        SemScalarExpr, SqlValueExpr, SubLinkExpr, SubQueryColumnRef, SystemColumnRef,
        TableColumnRef, TruthTestExpr, TypeCastExpr, WholeRowExpr,
    },
    select_statement::{ResultColumnCollection, SelectStatement},
    type_system::{FunctionOverloadCollection, FunctionParam, OverloadVariant, SimpleInferResult},
    update_statement::{
        UpdateAssignment, UpdateCursor, UpdateQueryValue, UpdateStatement, UpdateValue,
        ViewUpdateSupport, analyze_update_statement,
    },
    variable_set::{VariableSet, analyze_variable_set},
    view_like::ViewLike,
    wildcard_expansion::WildcardExpansion,
};
pub use crate::AstAndContextPair;

pub(crate) use self::{
    alter_table::AlterObjSubCommand,
    constraint::{ForeignKeyConstraint, UniqueConstraint},
    create_comp_type::{CompositeFieldCollection, CompositeFieldDefinition},
    create_index::IndexElementCollection,
    data_source::{AliasName, JoinUsingClause},
    delete_statement::{DeleteDestination, DeletePredicate},
    parameter_constraints::{ConstraintGraph, SignatureAlternative, array_cast_ranks},
    scalar_expr::{CaseExprAnalysisContext, WholeRowFields, grouping::ReferenceCapture},
    select_statement::{ResultColumn, SelectStatementNode},
    update_statement::{UpdateDestination, UpdatePredicate},
};
#[cfg(feature = "sqlite")]
use crate::{SqliteToken, syn::sqlite::Cmd};
use crate::{StringSpan, context::ParseContext};

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
    /// Resolved DROP TRIGGER, including explicit IF EXISTS no-op outcomes.
    DropTrigger(DropTrigger),
    /// Analyzed PostgreSQL CREATE RULE.
    CreateRule(Box<CreateRule>),
    /// Resolved PostgreSQL DROP RULE.
    DropRule(DropRule),
    /// A statement whose semantic resolution failed recoverably.
    /// Diagnostics are retained by the context; no catalog definition is applied.
    UnresolvedStatement,

    /// `alter owner` statement
    AlterOwner(AlterOwner),

    /// `alter sequence` statement
    AlterSequence(AlterSequence),
    /// Analyzed ALTER INDEX, including rejected and skipped alterations.
    AlterIndex(AlterIndex),

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
    pub(crate) fn walk_nested_command<V: self::traversal::SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        match self {
            Self::SelectStatement(query) => query.walk(visitor),
            Self::InsertStatement(insert) => insert.walk(visitor),
            Self::UpdateStatement(update) => {
                visitor.relation_identity(update.target().full_name());
                update.walk(visitor)
            }
            Self::DeleteStatement(delete) => {
                visitor.relation_identity(delete.target().full_name());
                delete.walk(visitor)
            }
            _ => unreachable!("only resolved common query/DML actions are traversed here"),
        }
    }

    /// Retain an analyzed rule definition.
    pub fn create_rule(rule: &CreateRule) -> Self {
        Self::CreateRule(Box::new(rule.clone()))
    }
    /// Inspect the statement-specific rule definition.
    pub fn rule(&self) -> Option<&CreateRule> {
        if let Self::CreateRule(rule) = self {
            Some(rule)
        } else {
            None
        }
    }
    pub(crate) const fn rule_returning(&self) -> Option<&ResultColumnCollection> {
        match self {
            Self::InsertStatement(statement) => Some(statement.result_columns()),
            Self::UpdateStatement(statement) => Some(statement.result_columns()),
            Self::DeleteStatement(statement) => Some(statement.result_columns()),
            _ => None,
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
    /// Inspect an analyzed ALTER TABLE statement and its applicability.
    pub const fn alter_table_statement(&self) -> Option<&AlterTable> {
        if let Self::AlterTable(statement) = self {
            Some(statement)
        } else {
            None
        }
    }
    /// Inspect a resolved SELECT/query statement, including a rule action.
    pub const fn select_statement(&self) -> Option<&SelectStatement> {
        if let Self::SelectStatement(query) = self {
            Some(query)
        } else {
            None
        }
    }
    /// Ordered output columns for SELECT or DML RETURNING.
    #[must_use]
    pub fn result_columns(&self) -> Option<&ResultColumnCollection> {
        if let Some(merge) = self.as_merge() {
            return Some(merge.result_columns());
        }
        if let Some(columns) = self.query_result_columns() {
            Some(columns)
        } else {
            self.rule_returning()
        }
    }

    pub(crate) fn query_result_columns(&self) -> Option<&ResultColumnCollection> {
        if let Self::SelectStatement(query) = self {
            Some(query.result_columns())
        } else {
            None
        }
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
    pub(crate) fn alter_index(index: &AlterIndex) -> Self {
        Self::AlterIndex(index.clone())
    }
    /// Inspect the completed ALTER INDEX analysis.
    pub const fn as_alter_index(&self) -> Option<&AlterIndex> {
        if let Self::AlterIndex(index) = self {
            Some(index)
        } else {
            None
        }
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
            Self::AlterIndex(index) => context.apply_alter_index(index),
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

pub(crate) use self::data_source::TableFunctionOutput;

pub(crate) use crate::stack::with_stack;
