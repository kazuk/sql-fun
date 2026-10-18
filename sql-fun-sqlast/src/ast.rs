//! Context-free SQL syntax shared by the parser frontends.
//!
//! Names in this module are spelling, not catalog bindings. In particular, type
//! names and function names do not imply that an object exists. Recursive edges
//! always stay in this representation, including expressions inside query clauses.
//! Converters consume newly built child nodes to avoid cloning recursive trees;
//! ordinary semantic factories borrow their inputs.

// Syntax retention intentionally precedes semantic support for some SQL clauses.
// Construction tests cover these fields even when resolution does not consume them.
#![allow(dead_code)]

mod ddl;
mod deparse;
mod document;
mod mutation;
mod postgres;
mod public;
mod resolve;
#[cfg(feature = "sqlite")]
mod sqlite;
#[cfg(all(feature = "sqlite", feature = "workaround_turso_9791"))]
mod sqlite_retention;
#[cfg(test)]
mod tests;
mod trigger;
mod validation;
mod view_write;

pub use self::deparse::DeparseError;
pub use self::mutation::{AstMutationError, CteDefinition, HaveWithClause, WithClauseMut};
pub use self::public::{CommonTableExpression, Statement, StatementKind, WithClause};

use sql_fun_core::IVec;

use self::document::{
    CmdType, JsonAggregate, JsonArgument, JsonArray, JsonArrayAggregate, JsonArrayQuery,
    JsonBehavior, JsonBehaviorType, JsonColumn, JsonEncoding, JsonExprOp, JsonFormat,
    JsonFormatType, JsonFunction, JsonKeyValue, JsonObject, JsonObjectAggregate, JsonOutput,
    JsonParse, JsonPath, JsonPredicate, JsonQuotes, JsonReturning, JsonScalar, JsonSerialize,
    JsonTableColumnType, JsonValue, JsonValueType, JsonWrapper, Merge, MergeMatchKind, MergeWhen,
    MultiAssignment, OverridingKind, Xml, XmlColumn, XmlExprOp, XmlOptionType, XmlSerialize,
};

pub(crate) use self::document::{JsonTable, XmlTable};

pub(crate) use self::ddl::{
    CompositeType, Domain, Function, FunctionParameter, Index, Schema, TableAs,
};
use crate::StringSpan;

#[derive(Clone, Debug, Default)]
pub(crate) struct TableExtras {
    unlogged: bool,
    inherits: Vec<Name>,
    of_type: Option<TypeName>,
    on_commit: Option<CommitAction>,
    tablespace: Option<String>,
    access_method: Option<String>,
    partition_spec: Option<PartitionSpec>,
    partition_bound: Option<PartitionBound>,
}
#[derive(Clone, Debug)]
pub(crate) struct PartitionSpec {
    strategy: String,
    keys: Vec<IndexExpression>,
}
#[derive(Clone, Debug)]
pub(crate) struct PartitionBound {
    strategy: String,
    is_default: bool,
    modulus: i32,
    remainder: i32,
    list: Vec<Expr>,
    lower: Vec<PartitionDatum>,
    upper: Vec<PartitionDatum>,
}
#[derive(Clone, Debug)]
pub(crate) enum PartitionDatum {
    Min,
    Max,
    Value(Expr),
}
#[derive(Clone, Debug, Default)]
pub(crate) struct ConstraintExtras {
    deferrable: bool,
    initially_deferred: bool,
    skip_validation: bool,
    no_inherit: bool,
    nulls_not_distinct: bool,
    include: Vec<String>,
    options: Vec<SyntaxOption>,
    index_name: Option<String>,
    tablespace: Option<String>,
    access_method: Option<String>,
    predicate: Option<Expr>,
    exclusions: Vec<ExclusionElement>,
    delete_columns: Vec<String>,
}
#[derive(Clone, Debug)]
pub(crate) struct ExclusionElement {
    expression: IndexExpression,
    operator: Name,
}

#[derive(Clone, Debug)]
pub(crate) struct Table {
    extras: TableExtras,
    name: Name,
    columns: Vec<Column>,
    constraints: Vec<TableConstraint>,
    temporary: bool,
    mode: crate::sem::CreateTableMode,
    query: Option<Query>,
    options: Vec<SyntaxOption>,
}

#[derive(Clone, Debug)]
pub(crate) struct Column {
    collation: Name,
    compression: Option<String>,
    storage: Option<String>,
    name: String,
    type_name: Option<TypeName>,
    constraints: Vec<TableConstraint>,
    not_null: Option<bool>,
}

#[derive(Clone, Debug)]
pub(crate) struct TableConstraint {
    extras: ConstraintExtras,
    name: Option<String>,
    keys: Vec<String>,
    constraint: ColumnConstraint,
    order: Vec<Order>,
    auto_increment: bool,
}

#[derive(Clone, Debug)]
pub(crate) enum ColumnConstraint {
    Nullability(
        bool,
        Option<crate::sem::ConflictClause>,
        Option<crate::sem::Validity>,
    ),
    Default(Expr),
    Check(Expr),
    PrimaryKey(
        Option<crate::sem::ConflictClause>,
        Option<crate::sem::Deferrable>,
    ),
    Unique(Option<crate::sem::ConflictClause>),
    Identity(crate::sem::IdentityColumnMode),
    Generated(Expr, Option<crate::sem::GeneratedColumnMode>),
    ForeignKey(ForeignReference),
    Exclusion,
    Deferrability(bool),
    Deferred,
    Immediate,
    Collate(String),
}

#[derive(Clone, Debug)]
pub(crate) struct ForeignReference {
    table: Name,
    columns: Vec<String>,
    match_type: Option<crate::sem::ForeignKeyMatchType>,
    delete_action: Option<crate::sem::ForeignKeyAction>,
    update_action: Option<crate::sem::ForeignKeyAction>,
    deferrability: Option<crate::sem::Deferrability>,
    validity: Option<crate::sem::Validity>,
    ignored_matches: Vec<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct AlterRelation {
    name: Name,
    kind: AlterRelationKind,
    commands: Vec<AlterCommand>,
    if_exists: bool,
    only: bool,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum AlterRelationKind {
    Table,
    ForeignTable,
    Index,
    CompositeType,
    View,
    MaterializedView,
    Sequence,
}

#[derive(Clone, Debug)]
pub(crate) enum AlterCommand {
    Postgres(Box<ddl::PostgresAlterCommand>),
    Owner(String),
    Default(String, Expr),
    Constraint(TableConstraint),
    Cluster(String),
    Rename(Name),
    AddColumn(Column),
    AlterColumn(String, Column),
    RenameColumn(String, String),
    DropColumn(String),
    Unsupported(String),
}

/// A context-free top-level SQL statement.
///
/// This is deliberately private while statement-family coverage is incomplete.
/// Parser frontends construct it without catalog access; semantic resolution is
/// performed only after construction has completed.
#[derive(Clone, Debug)]
pub(crate) enum StatementNode {
    Query(Query),
    Insert(Insert),
    Update(Update),
    Delete(Delete),
    View(View),
    ViewWrite(view_write::WriteDeclaration),
    CreateTrigger(CreateTrigger),
    DropTrigger(DropTrigger),
    Extension(Extension),
    VariableSet(VariableSet),
    EnumType(EnumType),
    Sequence(Sequence),
    DropTable(DropTable),
    AlterSequence(AlterSequence),
    Comment(Comment),
    AlterOwner(AlterOwner),
    CompositeType(CompositeType),
    Domain(Domain),
    Function(Function),
    Schema(Schema),
    Index(Index),
    TableAs(TableAs),
    Table(Table),
    AlterRelation(AlterRelation),
}

impl StatementNode {
    const fn query(value: Query) -> Self {
        Self::Query(value)
    }

    const fn insert(value: Insert) -> Self {
        Self::Insert(value)
    }

    const fn update(value: Update) -> Self {
        Self::Update(value)
    }

    const fn delete(value: Delete) -> Self {
        Self::Delete(value)
    }

    const fn create_trigger(value: CreateTrigger) -> Self {
        Self::CreateTrigger(value)
    }

    const fn drop_trigger(value: DropTrigger) -> Self {
        Self::DropTrigger(value)
    }

    const fn view_write(value: view_write::WriteDeclaration) -> Self {
        Self::ViewWrite(value)
    }

    const fn view(value: View) -> Self {
        Self::View(value)
    }

    const fn extension(value: Extension) -> Self {
        Self::Extension(value)
    }

    const fn variable_set(value: VariableSet) -> Self {
        Self::VariableSet(value)
    }

    const fn enum_type(value: EnumType) -> Self {
        Self::EnumType(value)
    }

    const fn sequence(value: Sequence) -> Self {
        Self::Sequence(value)
    }

    const fn drop_table(value: DropTable) -> Self {
        Self::DropTable(value)
    }

    const fn alter_sequence(value: AlterSequence) -> Self {
        Self::AlterSequence(value)
    }

    const fn comment(value: Comment) -> Self {
        Self::Comment(value)
    }

    const fn alter_owner(value: AlterOwner) -> Self {
        Self::AlterOwner(value)
    }

    const fn composite_type(value: CompositeType) -> Self {
        Self::CompositeType(value)
    }

    const fn alter_relation(value: AlterRelation) -> Self {
        Self::AlterRelation(value)
    }

    const fn domain(value: Domain) -> Self {
        Self::Domain(value)
    }

    const fn function(value: Function) -> Self {
        Self::Function(value)
    }
    const fn schema(value: Schema) -> Self {
        Self::Schema(value)
    }
    const fn index(value: Index) -> Self {
        Self::Index(value)
    }
    const fn table_as(value: TableAs) -> Self {
        Self::TableAs(value)
    }

    const fn table(value: Table) -> Self {
        Self::Table(value)
    }

    pub(crate) fn resolve(
        &self,
        context: &mut impl crate::sem::ParseContext,
    ) -> Result<crate::sem::SemAst, crate::sem::AnalysisError> {
        match self {
            Self::Query(query) => query
                .resolve(context)
                .map(|query| crate::sem::SemAst::select(&query)),
            Self::Insert(insert) => insert
                .resolve(context)
                .map(|insert| crate::sem::SemAst::insert(&insert)),
            Self::Update(update) => update
                .resolve(context)
                .map(|update| crate::sem::SemAst::update(&update)),
            Self::Delete(delete) => delete
                .resolve(context)
                .map(|delete| crate::sem::SemAst::delete(&delete)),
            Self::DropTrigger(_) => todo!("DROP TRIGGER semantic analysis: #555"),
            Self::CreateTrigger(_) => todo!("CREATE TRIGGER semantic analysis: #602 / #555"),
            Self::ViewWrite(_) => crate::sem::AnalysisError::raise_unexpected_input(
                "view write declaration requires catalog application",
            ),
            Self::View(_) => crate::sem::AnalysisError::raise_unexpected_input(
                "view requires catalog application",
            ),
            Self::Extension(_) => crate::sem::AnalysisError::raise_unexpected_input(
                "extension requires catalog application",
            ),
            Self::VariableSet(_) => crate::sem::AnalysisError::raise_unexpected_input(
                "variable set requires catalog application",
            ),
            Self::EnumType(_) => crate::sem::AnalysisError::raise_unexpected_input(
                "enum type requires catalog application",
            ),
            Self::Sequence(_) => crate::sem::AnalysisError::raise_unexpected_input(
                "sequence requires catalog application",
            ),
            Self::DropTable(_) => crate::sem::AnalysisError::raise_unexpected_input(
                "drop table requires catalog application",
            ),
            Self::AlterSequence(_) => crate::sem::AnalysisError::raise_unexpected_input(
                "alter sequence requires catalog application",
            ),
            Self::Comment(_) => crate::sem::AnalysisError::raise_unexpected_input(
                "comment requires catalog application",
            ),
            Self::AlterOwner(_) => {
                Ok(crate::sem::SemAst::AlterOwner(crate::sem::AlterOwner::new()))
            }
            Self::CompositeType(composite_type) => composite_type
                .resolve()
                .map(crate::sem::SemAst::CreateCompType),
            Self::AlterRelation(_) => crate::sem::AnalysisError::raise_unexpected_input(
                "statement requires catalog application",
            ),
            Self::Function(function) => function
                .resolve(context)
                .map(|function| crate::sem::SemAst::create_function(&function)),
            Self::Index(index) => index
                .resolve(context)
                .map(|index| crate::sem::SemAst::from_index_resolution(&index)),
            Self::TableAs(_) => crate::sem::AnalysisError::raise_unexpected_input(
                "table as requires catalog application",
            ),
            Self::Table(_) => {
                crate::sem::AnalysisError::raise_unexpected_input("table requires source metadata")
            }
            Self::Schema(_) => crate::sem::AnalysisError::raise_unexpected_input(
                "schema requires catalog application",
            ),
            Self::Domain(domain) => domain
                .resolve(context)
                .map(|domain| crate::sem::SemAst::create_domain(&domain)),
        }
        .or_else(|error| error.recover_view_metadata(context))
    }

    pub(crate) fn resolve_and_apply<TContext>(
        &self,
        context: TContext,
    ) -> Result<crate::AstAndContextPair<TContext>, crate::sem::AnalysisError>
    where
        TContext: crate::sem::ParseContext,
    {
        let mut context = context;
        context.set_parameter_constraints(&crate::sem::ParameterConstraints::default());
        match self {
            Self::View(view) => view.resolve_and_apply(context),
            Self::ViewWrite(change) => change.resolve_and_apply(context),
            Self::Extension(extension) => extension.apply(context),
            Self::VariableSet(variable_set) => variable_set.resolve_and_apply(context),
            Self::EnumType(enum_type) => enum_type.apply(context),
            Self::Sequence(sequence) => sequence.apply(context),
            Self::DropTable(drop_table) => drop_table.apply(context),
            Self::AlterSequence(alter_sequence) => alter_sequence.apply(context),
            Self::Comment(comment) => comment.apply(context),
            Self::AlterOwner(_) => Ok(crate::AstAndContextPair::new(
                crate::sem::SemAst::AlterOwner(crate::sem::AlterOwner::new()),
                context,
            )),
            Self::CompositeType(composite_type) => composite_type.resolve_and_apply(context),
            Self::AlterRelation(statement) => statement.resolve_and_apply(context),
            Self::Domain(domain) => domain.resolve_and_apply(context),
            Self::Function(function) => function.resolve_and_apply(context),
            Self::Schema(schema) => schema.resolve_and_apply(context),
            Self::Index(index) => index.resolve_and_apply(context),
            Self::TableAs(table_as) => table_as.resolve_and_apply(context),
            Self::Table(table) => table.resolve_and_apply(context),
            _ => {
                let outcome = self.resolve(&mut context);
                let statement = outcome?;
                Ok(crate::AstAndContextPair::new(statement, context))
            }
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct AlterOwner {
    object_kind: Option<crate::sem::ObjectKind>,
    relation: Option<Name>,
    object: Option<ObjectTarget>,
    owner: Role,
}

/// A named database object, optionally identified by its argument type list.
/// `None` distinguishes an unspecified signature from an explicitly empty `()`.
#[derive(Clone, Debug)]
pub(crate) struct ObjectTarget {
    name: Vec<String>,
    argument_types: Option<Vec<ObjectArgument>>,
    cast: Option<CastTarget>,
    type_name: Option<TypeName>,
    domain_constraint: Option<DomainConstraintTarget>,
    transform: Option<TransformTarget>,
}

#[derive(Clone, Debug)]
pub(crate) struct CastTarget {
    source: TypeName,
    target: TypeName,
}

#[derive(Clone, Debug)]
pub(crate) struct DomainConstraintTarget {
    domain: TypeName,
    constraint: String,
}

#[derive(Clone, Debug)]
pub(crate) struct TransformTarget {
    type_name: TypeName,
    language: String,
}

#[derive(Clone, Debug)]
pub(crate) struct ObjectArgument {
    mode: Option<String>,
    name: Option<String>,
    type_name: Option<TypeName>,
}

#[derive(Clone, Debug)]
pub(crate) struct Role {
    kind: u32,
    name: String,
}

impl AlterOwner {
    const fn new(
        object_kind: Option<crate::sem::ObjectKind>,
        relation: Option<Name>,
        object: Option<ObjectTarget>,
        owner: Role,
    ) -> Self {
        Self {
            object_kind,
            relation,
            object,
            owner,
        }
    }
}

impl Role {
    fn new(kind: u32, name: &str) -> Self {
        Self {
            kind,
            name: name.to_owned(),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Comment {
    target: ObjectTarget,
    object_kind: crate::sem::ObjectKind,
    text: String,
}

impl Comment {
    fn new(target: ObjectTarget, object_kind: crate::sem::ObjectKind, text: &str) -> Self {
        Self {
            target,
            object_kind,
            text: text.to_owned(),
        }
    }

    fn apply<TContext>(
        &self,
        context: TContext,
    ) -> Result<crate::AstAndContextPair<TContext>, crate::sem::AnalysisError>
    where
        TContext: crate::sem::ParseContext,
    {
        let target = self.target.semantic_name();
        let comment = crate::sem::Comment::new(&target, self.object_kind, &self.text);
        let context = context.apply_comment(&comment)?;
        Ok(crate::AstAndContextPair::new(
            crate::sem::SemAst::Comment(comment),
            context,
        ))
    }
}

impl ObjectTarget {
    fn semantic_name(&self) -> String {
        if let Some(cast) = &self.cast {
            format!(
                "{} AS {}",
                cast.source.name.0.join("."),
                cast.target.name.0.join(".")
            )
        } else if let Some(type_name) = &self.type_name {
            type_name.name.0.join(".")
        } else if let Some(target) = &self.domain_constraint {
            format!(
                "{} ON DOMAIN {}",
                target.constraint,
                target.domain.name.0.join(".")
            )
        } else if let Some(target) = &self.transform {
            format!(
                "FOR {} LANGUAGE {}",
                target.type_name.name.0.join("."),
                target.language
            )
        } else {
            self.name.join(".")
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct AlterSequence {
    name: Name,
    options: Vec<SyntaxOption>,
    for_identity: bool,
    missing_ok: bool,
}

impl AlterSequence {
    const fn new(
        name: Name,
        options: Vec<SyntaxOption>,
        for_identity: bool,
        missing_ok: bool,
    ) -> Self {
        Self {
            name,
            options,
            for_identity,
            missing_ok,
        }
    }

    fn apply<TContext>(
        &self,
        context: TContext,
    ) -> Result<crate::AstAndContextPair<TContext>, crate::sem::AnalysisError>
    where
        TContext: crate::sem::ParseContext,
    {
        let alter_sequence = crate::sem::AlterSequence::new(&self.name.full_name()?, &[]);
        let context = context.apply_alter_sequence(&alter_sequence)?;
        Ok(crate::AstAndContextPair::new(
            crate::sem::SemAst::AlterSequence(alter_sequence),
            context,
        ))
    }
}

#[derive(Clone, Debug)]
pub(crate) struct DropTable {
    names: Vec<Name>,
    if_exists: bool,
    cascade: bool,
    concurrent: bool,
}

impl DropTable {
    const fn new(names: Vec<Name>, if_exists: bool, cascade: bool, concurrent: bool) -> Self {
        Self {
            names,
            if_exists,
            cascade,
            concurrent,
        }
    }

    fn apply<TContext>(
        &self,
        mut context: TContext,
    ) -> Result<crate::AstAndContextPair<TContext>, crate::sem::AnalysisError>
    where
        TContext: crate::sem::ParseContext,
    {
        let mut names = Vec::new();
        for name in &self.names {
            let table_name = crate::sem::TableName::from(name.full_name()?);
            if context.get_table(&table_name).is_none() {
                context.report_analysis_problem(crate::sem::AnalysisProblem::table_not_found(
                    &table_name,
                ))?;
            }
            names.push(table_name);
        }
        let drop_table = crate::sem::DropTable::new(&names, self.if_exists, self.cascade);
        context = context.apply_drop_table(&drop_table)?;
        Ok(crate::AstAndContextPair::new(
            crate::sem::SemAst::DropTable(drop_table),
            context,
        ))
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Sequence {
    name: Name,
    options: Vec<SyntaxOption>,
    owner_id: u32,
    for_identity: bool,
    if_not_exists: bool,
}

impl Sequence {
    const fn new(
        name: Name,
        options: Vec<SyntaxOption>,
        owner_id: u32,
        for_identity: bool,
        if_not_exists: bool,
    ) -> Self {
        Self {
            name,
            options,
            owner_id,
            for_identity,
            if_not_exists,
        }
    }

    fn apply<TContext>(
        &self,
        context: TContext,
    ) -> Result<crate::AstAndContextPair<TContext>, crate::sem::AnalysisError>
    where
        TContext: crate::sem::ParseContext,
    {
        let sequence = crate::sem::CreateSequence::new(&self.name.full_name()?);
        let context = context.apply_create_sequence(&sequence)?;
        Ok(crate::AstAndContextPair::new(
            crate::sem::SemAst::CreateSequence(sequence),
            context,
        ))
    }
}

#[derive(Clone, Debug)]
pub(crate) struct EnumType {
    name: Name,
    values: Vec<String>,
}

impl EnumType {
    const fn new(name: Name, values: Vec<String>) -> Self {
        Self { name, values }
    }

    fn apply<TContext>(
        &self,
        context: TContext,
    ) -> Result<crate::AstAndContextPair<TContext>, crate::sem::AnalysisError>
    where
        TContext: crate::sem::ParseContext,
    {
        let enum_type = crate::sem::CreateEnum::new(&self.name.full_name()?, &self.values);
        let context = context.apply_create_enum(&enum_type)?;
        Ok(crate::AstAndContextPair::new(
            crate::sem::SemAst::CreateEnum(enum_type),
            context,
        ))
    }
}

#[derive(Clone, Debug)]
pub(crate) struct VariableSet {
    kind: VariableSetKind,
    is_local: bool,
    name: String,
    arguments: Vec<Expr>,
    transaction_options: Option<Vec<SyntaxOption>>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum VariableSetKind {
    Value,
    Default,
    Current,
    Multi,
    Reset,
    ResetAll,
}

impl VariableSet {
    fn new(name: &str, arguments: Vec<Expr>) -> Self {
        Self {
            kind: VariableSetKind::Value,
            is_local: false,
            name: name.to_owned(),
            arguments,
            transaction_options: None,
        }
    }

    fn resolve_and_apply<TContext>(
        &self,
        mut context: TContext,
    ) -> Result<crate::AstAndContextPair<TContext>, crate::sem::AnalysisError>
    where
        TContext: crate::sem::ParseContext,
    {
        let [argument] = self.arguments.as_slice() else {
            return crate::sem::AnalysisError::raise_unexpected_input(
                "VariableSetStmt.args len!=1",
            );
        };
        let outcome = argument.resolve(&mut context, &crate::sem::ScalarAnalysisContext::default());
        let value = outcome?;
        let variable_set = crate::sem::VariableSet::new(&self.name, &value);
        let context = context.apply_variable_set(&variable_set)?;
        Ok(crate::AstAndContextPair::new(
            crate::sem::SemAst::VariableSet(variable_set),
            context,
        ))
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Extension {
    if_not_exists: bool,
    options: Vec<SyntaxOption>,
    name: String,
}

impl Extension {
    fn new(name: &str) -> Self {
        Self {
            if_not_exists: false,
            options: Vec::new(),
            name: name.to_owned(),
        }
    }

    fn apply<TContext>(
        &self,
        context: TContext,
    ) -> Result<crate::AstAndContextPair<TContext>, crate::sem::AnalysisError>
    where
        TContext: crate::sem::ParseContext,
    {
        let extension = crate::sem::CreateExtension::from_name(&self.name);
        let context = context.apply_create_extension(&extension)?;
        Ok(crate::AstAndContextPair::new(
            crate::sem::SemAst::CreateExtension(extension),
            context,
        ))
    }
}

#[derive(Clone, Debug)]
pub(crate) struct View {
    name: Name,
    aliases: Vec<String>,
    query: Query,
    replace: bool,
    options: Vec<SyntaxOption>,
    check_option: ViewCheckOption,
    temporary: bool,
    if_not_exists: bool,
    materialized: bool,
}

impl View {
    #[expect(clippy::too_many_arguments)]
    const fn new(
        name: Name,
        aliases: Vec<String>,
        query: Query,
        replace: bool,
        options: Vec<SyntaxOption>,
        check_option: ViewCheckOption,
        temporary: bool,
        if_not_exists: bool,
        materialized: bool,
    ) -> Self {
        Self {
            name,
            aliases,
            query,
            replace,
            options,
            check_option,
            temporary,
            if_not_exists,
            materialized,
        }
    }

    fn resolve_and_apply<TContext>(
        &self,
        context: TContext,
    ) -> Result<crate::AstAndContextPair<TContext>, crate::sem::AnalysisError>
    where
        TContext: crate::sem::ParseContext,
    {
        let mut context = context;
        let Some(name) = self.write_name(&mut context)? else {
            return Ok(crate::AstAndContextPair::new(
                crate::sem::SemAst::unresolved_statement(),
                context,
            ));
        };
        if context.get_view_impl(&name).is_some() {
            if self.if_not_exists {
                return Ok(crate::AstAndContextPair::new(
                    crate::sem::SemAst::unresolved_statement(),
                    context,
                ));
            }
            if !self.replace {
                context.report_analysis_problem(crate::sem::AnalysisProblem::view(
                    &crate::sem::problem::ViewProblem::duplicate_object(&name, &name.to_string()),
                ))?;
                return Ok(crate::AstAndContextPair::new(
                    crate::sem::SemAst::unresolved_statement(),
                    context,
                ));
            }
        }
        let outcome = self.query.resolve(&mut context);
        let mut query = outcome?;
        query.rename_columns(&self.aliases);
        let Some(info) = self.write_info(&mut context, &query, &name)? else {
            return Ok(crate::AstAndContextPair::new(
                crate::sem::SemAst::unresolved_statement(),
                context,
            ));
        };
        let view = crate::sem::CreateView::new(&name, &query, &[], &None, self.materialized);
        let context = context
            .apply_create_view(&view)?
            .apply_view_write_info(&name, &info)?;
        Ok(crate::AstAndContextPair::new(
            crate::sem::SemAst::CreateView(view),
            context,
        ))
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum ViewCheckOption {
    None,
    Local,
    Cascaded,
}

pub(crate) trait TryFromWithTokens<T>
where
    Self: Sized,
{
    type Error;

    fn try_from_with_tokens(
        value: T,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Self, Self::Error>;
}

pub(crate) trait TryFromWithTokensOpt<T>
where
    Self: Sized,
{
    type Error;

    fn try_from_with_tokens_opt(
        value: T,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Option<Self>, Self::Error>;
}

pub(crate) trait TryFromOpt<T>: TryFrom<T> {
    fn try_from_opt(value: Option<T>) -> Result<Option<Self>, <Self as TryFrom<T>>::Error>
    where
        Self: Sized,
    {
        value.map(Self::try_from).transpose()
    }
}

impl<T, U> TryFromOpt<T> for U where U: TryFrom<T> {}

pub(crate) trait TryFromList<T>: Sized {
    type Error;

    fn try_from_list(values: &[T]) -> Result<Vec<Self>, Self::Error>;
}

impl<T, U, E> TryFromList<T> for U
where
    for<'a> U: TryFrom<&'a T, Error = E>,
{
    type Error = E;

    fn try_from_list(values: &[T]) -> Result<Vec<Self>, Self::Error> {
        values.iter().map(Self::try_from).collect()
    }
}

pub(crate) trait FromList<T>: Sized {
    fn from_list(values: &[T]) -> Vec<Self>;
}

impl<T, U> FromList<T> for U
where
    for<'a> U: From<&'a T>,
{
    fn from_list(values: &[T]) -> Vec<Self> {
        values.iter().map(Self::from).collect()
    }
}

pub(crate) trait FromOpt<T>: Sized {
    fn from_opt(value: Option<T>) -> Option<Self>;
}

impl<T, U> FromOpt<T> for U
where
    U: From<T>,
{
    fn from_opt(value: Option<T>) -> Option<Self> {
        value.map(Self::from)
    }
}

pub(crate) trait TryCollectFromWithTokens<T>
where
    Self: Sized,
{
    type Error;

    fn try_collect_from_with_tokens(
        nodes: T,
        tokens: &IVec<crate::syn::ScanToken>,
    ) -> Result<Vec<Self>, Self::Error>;
}

/// SQL dialect explicitly selected for parsing and AST editing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Dialect {
    /// PostgreSQL SQL syntax.
    Postgres,
    /// SQLite SQL syntax; parsing requires the `sqlite` feature.
    Sqlite,
}

impl Dialect {
    /// Select PostgreSQL.
    pub const fn postgres() -> Self {
        Self::Postgres
    }
    /// Select SQLite.
    pub const fn sqlite() -> Self {
        Self::Sqlite
    }
}

impl From<&::sql_fun_core::SqlFunMetadata> for Dialect {
    fn from(metadata: &::sql_fun_core::SqlFunMetadata) -> Self {
        match metadata.sql_dialect() {
            ::sql_fun_core::SqlDialect::PostgreSQL => Self::postgres(),
            #[cfg(feature = "sqlite")]
            ::sql_fun_core::SqlDialect::SQLite => Self::sqlite(),
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Name(Vec<String>);

impl Name {
    fn from_parts(parts: &[String]) -> Self {
        Self(parts.to_vec())
    }

    fn local(value: &str) -> Self {
        Self(vec![value.to_owned()])
    }
}

#[derive(Clone, Debug)]
pub(crate) struct TypeName {
    name: Name,
    modifiers: Vec<Expr>,
    array_bounds: Vec<Option<i32>>,
    set_of: bool,
    percent_type: bool,
    sqlite_name_snapshot: Option<Name>,
    sqlite_parts: Vec<SqliteTypeNamePart>,
    sqlite_modifiers: Option<SqliteTypeModifiers>,
    location: Option<StringSpan>,
}

#[derive(Clone, Debug)]
pub(crate) struct SqliteTypeNamePart {
    name: String,
    quote: Option<char>,
    leading_trivia: Vec<SqliteTypeNameTrivia>,
    trailing_trivia: Vec<SqliteTypeNameTrivia>,
}

#[derive(Clone, Debug)]
pub(crate) struct SqliteTypeModifiers {
    values: Vec<SqliteTypeModifier>,
    ast_spellings: Vec<String>,
    trailing_trivia: Vec<SqliteTypeNameTrivia>,
}

#[derive(Clone, Debug)]
pub(crate) struct SqliteTypeModifier {
    leading_trivia: Vec<SqliteTypeNameTrivia>,
    spelling: String,
    trailing_trivia: Vec<SqliteTypeNameTrivia>,
}

#[derive(Clone, Debug)]
pub(crate) enum SqliteTypeNameTrivia {
    Whitespace(String),
    BlockComment(String),
    LineComment(String),
}

#[derive(Clone, Debug)]
pub(crate) enum Literal {
    Null,
    Integer(i32),
    Numeric(String),
    String(String),
    Boolean(bool),
    BitString(String),
    Blob(String),
    Keyword(String),
}

#[derive(Clone, Debug)]
pub(crate) enum Expr {
    Literal(Literal),
    Column(Name),
    Star(Name),
    Parameter(Parameter),
    Operator(Box<OperatorExpr>),
    Boolean(Box<BooleanExpr>),
    NullTest(Box<NullTest>),
    TruthTest(Box<TruthTest>),
    Cast(Box<Cast>),
    Collate(Box<Collate>),
    Call(Box<Call>),
    Case(Box<Case>),
    Subquery(Box<Subquery>),
    Array(Vec<Self>),
    Row(Vec<Self>),
    List(Vec<Self>),
    Parenthesized(Vec<Self>),
    Indirection(Box<Indirection>),
    NamedArgument(Box<NamedArgument>),
    SqlValue(SqlValue),
    Default,
    Raise(Box<Raise>),
    InTable(Box<InTable>),
    Document(document::DocumentExpr),
    Grouping(Vec<Self>),
    CurrentOf(CurrentOf),
    MergeAction,
}

impl Literal {
    const fn null() -> Self {
        Self::Null
    }
}

impl Expr {
    pub(in crate::ast) const fn null() -> Self {
        Self::Literal(Literal::null())
    }
}

#[derive(Clone, Debug)]
pub(crate) struct CurrentOf {
    name: String,
    parameter: i32,
}

#[derive(Clone, Debug)]
pub(crate) struct Parameter {
    position: u32,
    name: Option<String>,
    type_name: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct InTable {
    argument: Expr,
    negated: bool,
    table: Name,
    arguments: Vec<Expr>,
}

#[derive(Clone, Debug)]
pub(crate) struct OperatorExpr {
    name: Name,
    kind: OperatorKind,
    left: Option<Expr>,
    right: Option<Expr>,
    escape: Option<Expr>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum OperatorKind {
    Ordinary,
    Any,
    All,
    Distinct,
    NotDistinct,
    NullIf,
    In,
    NotIn,
    Like,
    NotLike,
    ILike,
    Similar,
    Between,
    NotBetween,
    BetweenSymmetric,
    NotBetweenSymmetric,
}

#[derive(Clone, Debug)]
pub(crate) struct BooleanExpr {
    operator: BooleanOperator,
    arguments: Vec<Expr>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum BooleanOperator {
    And,
    Or,
    Not,
}

#[derive(Clone, Debug)]
pub(crate) struct NullTest {
    argument: Expr,
    negated: bool,
    row: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct TruthTest {
    argument: Expr,
    value: Option<bool>,
    negated: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct Cast {
    argument: Expr,
    target: TypeName,
}

#[derive(Clone, Debug)]
pub(crate) struct Collate {
    argument: Expr,
    name: Name,
}

#[derive(Clone, Debug)]
pub(crate) struct Call {
    sql_syntax: bool,
    kind: CallKind,
    name: Name,
    arguments: Vec<Expr>,
    star: bool,
    distinct: bool,
    variadic: bool,
    within_group: bool,
    within_order: Vec<Order>,
    order: Vec<Order>,
    filter: Option<Expr>,
    over: Option<Window>,
    location: Option<StringSpan>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum CallKind {
    Ordinary,
    Coalesce,
    MinMax,
}

impl CallKind {
    const fn ordinary() -> Self {
        Self::Ordinary
    }
    fn special(name: &str) -> Self {
        if name == "coalesce" {
            Self::Coalesce
        } else {
            Self::MinMax
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Case {
    argument: Option<Expr>,
    arms: Vec<CaseArm>,
    otherwise: Option<Expr>,
}

#[derive(Clone, Debug)]
pub(crate) struct CaseArm {
    condition: Expr,
    result: Expr,
}

#[derive(Clone, Debug)]
pub(crate) struct Subquery {
    query: Query,
    kind: SubqueryKind,
    test: Option<Expr>,
    operator: Name,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum SubqueryKind {
    Scalar,
    Exists,
    Any,
    All,
    Row,
    Array,
    MultiExpression,
}

#[derive(Clone, Debug)]
pub(crate) struct Indirection {
    argument: Expr,
    items: Vec<Access>,
}

#[derive(Clone, Debug)]
pub(crate) enum Access {
    Field(String),
    Star,
    Index(Expr),
    Slice(Slice),
}

#[derive(Clone, Debug)]
pub(crate) struct Slice {
    lower: Option<Expr>,
    upper: Option<Expr>,
}

#[derive(Clone, Debug)]
pub(crate) struct NamedArgument {
    name: String,
    argument: Expr,
}

#[derive(Clone, Debug)]
pub(crate) struct SqlValue {
    name: String,
    precision: Option<i32>,
}

#[derive(Clone, Debug)]
pub(crate) struct Raise {
    action: RaiseAction,
    message: Option<Expr>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum RaiseAction {
    Rollback,
    Abort,
    Fail,
    Ignore,
    Replace,
}

#[derive(Clone, Debug)]
pub(crate) struct Query {
    with: With,
    body: QueryBody,
    order: Vec<Order>,
    limit: Option<Expr>,
    offset: Option<Expr>,
    with_ties: bool,
    locks: Vec<Lock>,
}

#[derive(Clone, Debug)]
pub(crate) enum QueryBody {
    Select(Box<Select>),
    Values(Vec<Vec<Expr>>),
    Set(Box<SetQuery>),
}

#[derive(Clone, Debug)]
pub(crate) struct SetQuery {
    operation: SetOperation,
    all: bool,
    left: Query,
    right: Query,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum SetOperation {
    Union,
    Intersect,
    Except,
}

#[derive(Clone, Debug)]
pub(crate) struct Select {
    into: Option<Into>,
    distinct: Option<Vec<Expr>>,
    targets: Vec<Target>,
    from: Vec<FromItem>,
    predicate: Option<Expr>,
    group: Vec<Grouping>,
    group_distinct: bool,
    having: Option<Expr>,
    windows: Vec<Window>,
}

#[derive(Clone, Debug)]
pub(crate) struct Target {
    expression: Expr,
    alias: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) enum Grouping {
    Expression(Expr),
    Empty,
    Rollup(Vec<Self>),
    Cube(Vec<Self>),
    Sets(Vec<Self>),
    Simple(Vec<Self>),
}

#[derive(Clone, Debug)]
pub(crate) struct Order {
    expression: Expr,
    descending: Option<bool>,
    nulls_first: Option<bool>,
    using: Option<Name>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct With {
    recursive: bool,
    entries: Vec<Cte>,
}

#[derive(Clone, Debug)]
pub(crate) struct Cte {
    name: String,
    columns: Vec<CteColumn>,
    materialized: Option<bool>,
    body: CteBody,
    search: Option<Search>,
    cycle: Option<Cycle>,
}

#[derive(Clone, Debug)]
pub(crate) struct CteColumn {
    name: String,
    collation: Option<String>,
    descending: Option<bool>,
}

impl Cte {
    fn column_names(&self) -> Vec<String> {
        self.columns
            .iter()
            .map(|column| column.name.clone())
            .collect()
    }
}

impl CteColumn {
    fn from_names(names: &[String]) -> Vec<Self> {
        let mut columns = Vec::new();
        for name in names {
            columns.push(Self {
                name: name.clone(),
                collation: None,
                descending: None,
            });
        }
        columns
    }
}

#[derive(Clone, Debug)]
pub(crate) enum CteBody {
    Query(Box<Query>),
    Insert(Box<Insert>),
    Update(Box<Update>),
    Delete(Box<Delete>),
    Merge(Box<Merge>),
}

impl CteBody {
    pub(crate) fn query(query: Query) -> Self {
        Self::Query(Box::new(query))
    }

    pub(crate) fn insert(insert: Insert) -> Self {
        Self::Insert(Box::new(insert))
    }

    pub(crate) fn update(update: Update) -> Self {
        Self::Update(Box::new(update))
    }

    pub(crate) fn delete(delete: Delete) -> Self {
        Self::Delete(Box::new(delete))
    }

    pub(crate) fn merge(merge: Merge) -> Self {
        Self::Merge(Box::new(merge))
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Search {
    columns: Vec<String>,
    breadth_first: bool,
    sequence: String,
}

#[derive(Clone, Debug)]
pub(crate) struct Cycle {
    columns: Vec<String>,
    mark: String,
    value: Option<Expr>,
    default: Option<Expr>,
    path: String,
}

#[derive(Clone, Debug)]
pub(crate) struct Alias {
    name: String,
    columns: Vec<String>,
}

#[derive(Clone, Debug)]
pub(crate) enum FromItem {
    Relation(Relation),
    Query(Box<DerivedTable>),
    Join(Box<Join>),
    Functions(TableFunctions),
    Group(Box<FromGroup>),
    Sample(Box<TableSample>),
    Xml(Box<XmlTable>),
    Json(Box<JsonTable>),
}

impl FromItem {
    pub(crate) const fn relation(relation: Relation) -> Self {
        Self::Relation(relation)
    }

    pub(crate) fn query(query: DerivedTable) -> Self {
        Self::Query(Box::new(query))
    }

    pub(crate) fn join(join: Join) -> Self {
        Self::Join(Box::new(join))
    }

    pub(crate) const fn functions(functions: TableFunctions) -> Self {
        Self::Functions(functions)
    }

    pub(crate) fn group(group: FromGroup) -> Self {
        Self::Group(Box::new(group))
    }

    pub(crate) fn sample(sample: TableSample) -> Self {
        Self::Sample(Box::new(sample))
    }

    pub(crate) fn xml(table: XmlTable) -> Self {
        Self::Xml(Box::new(table))
    }

    pub(crate) fn json(table: JsonTable) -> Self {
        Self::Json(Box::new(table))
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Relation {
    name: Name,
    alias: Option<Alias>,
    inherit: bool,
    persistence: String,
    indexed: Option<String>,
    not_indexed: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct DerivedTable {
    query: Query,
    alias: Option<Alias>,
    lateral: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct FromGroup {
    items: Vec<FromItem>,
    alias: Option<Alias>,
}

#[derive(Clone, Debug)]
pub(crate) struct Join {
    left: FromItem,
    right: FromItem,
    kind: JoinKind,
    natural: bool,
    condition: Option<Expr>,
    using: Vec<String>,
    using_alias: Option<Alias>,
    alias: Option<Alias>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum JoinKind {
    Inner,
    Left,
    Right,
    Full,
    Cross,
}

#[derive(Clone, Debug)]
pub(crate) struct TableFunctions {
    calls: Vec<TableFunction>,
    alias: Option<Alias>,
    lateral: bool,
    ordinality: bool,
    rows_from: bool,
    columns: Vec<TypedColumn>,
}

#[derive(Clone, Debug)]
pub(crate) struct TableFunction {
    expression: Expr,
    columns: Vec<TypedColumn>,
}

impl TableFunctions {
    pub(crate) fn single(call: TableFunction, alias: Option<Alias>) -> Self {
        Self {
            calls: vec![call],
            alias,
            lateral: false,
            ordinality: false,
            rows_from: false,
            columns: Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct TypedColumn {
    name: String,
    type_name: TypeName,
    collation: Option<Name>,
}

#[derive(Clone, Debug)]
pub(crate) struct TableSample {
    relation: FromItem,
    method: Name,
    arguments: Vec<Expr>,
    repeatable: Option<Expr>,
}

#[derive(Clone, Debug)]
pub(crate) struct Window {
    reference_only: bool,
    name: Option<String>,
    reference: Option<String>,
    partition: Vec<Expr>,
    order: Vec<Order>,
    frame: Option<Frame>,
}

#[derive(Clone, Debug)]
pub(crate) struct Frame {
    mode: FrameMode,
    start: FrameBound,
    end: Option<FrameBound>,
    exclusion: Option<FrameExclusion>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum FrameMode {
    Rows,
    Range,
    Groups,
}

#[derive(Clone, Debug)]
pub(crate) enum FrameBound {
    UnboundedPreceding,
    Preceding(Expr),
    CurrentRow,
    Following(Expr),
    UnboundedFollowing,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum FrameExclusion {
    CurrentRow,
    Group,
    Ties,
    NoOthers,
}

#[derive(Clone, Debug)]
pub(crate) struct Lock {
    relations: Vec<Name>,
    strength: Option<LockStrength>,
    wait: Option<LockWait>,
}

#[derive(Clone, Debug)]
pub(crate) struct Assignment {
    name: String,
    indirection: Vec<Access>,
    value: Option<Expr>,
}

#[derive(Clone, Debug)]
pub(crate) struct Insert {
    with: With,
    relation: Relation,
    columns: Vec<Assignment>,
    source: Option<Query>,
    overriding: Option<OverridingKind>,
    conflict: Option<Conflict>,
    or_conflict: Option<crate::sem::ConflictClause>,
    returning: Vec<Target>,
}

#[derive(Clone, Debug)]
pub(crate) struct Conflict {
    action: Option<ConflictAction>,
    inference: Vec<IndexExpression>,
    inference_predicate: Option<Expr>,
    constraint: Option<String>,
    assignments: Vec<Assignment>,
    predicate: Option<Expr>,
    next: Option<Box<Self>>,
}

#[derive(Clone, Debug)]
pub(crate) struct IndexExpression {
    name: Option<String>,
    alias: Option<String>,
    expression: Option<Expr>,
    collation: Name,
    operator_class: Name,
    options: Vec<SyntaxOption>,
    descending: Option<bool>,
    using_order: bool,
    nulls_first: Option<bool>,
}

#[derive(Clone, Debug)]
pub(crate) struct Into {
    relation: Relation,
    columns: Vec<String>,
    access_method: Option<String>,
    options: Vec<SyntaxOption>,
    on_commit: Option<CommitAction>,
    tablespace: Option<String>,
    skip_data: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct SyntaxOption {
    namespace: Option<String>,
    name: String,
    value: Option<SyntaxValue>,
}

#[derive(Clone, Debug)]
pub(crate) enum SyntaxValue {
    Expression(Expr),
    Type(TypeName),
}

#[derive(Clone, Debug)]
pub(crate) struct Update {
    with: With,
    relation: Relation,
    assignments: Vec<Assignment>,
    from: Vec<FromItem>,
    predicate: Option<Expr>,
    returning: Vec<Target>,
    or_conflict: Option<crate::sem::ConflictClause>,
}

#[derive(Clone, Debug)]
pub(crate) struct Delete {
    with: With,
    relation: Relation,
    using: Vec<FromItem>,
    predicate: Option<Expr>,
    returning: Vec<Target>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum LockStrength {
    None,
    KeyShare,
    Share,
    NoKeyUpdate,
    Update,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum LockWait {
    Block,
    Skip,
    Error,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum ConflictAction {
    None,
    Nothing,
    Update,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum CommitAction {
    None,
    PreserveRows,
    DeleteRows,
    Drop,
}

impl BooleanOperator {
    const fn and() -> Self {
        Self::And
    }
    const fn or() -> Self {
        Self::Or
    }
    const fn not() -> Self {
        Self::Not
    }
}

impl OperatorKind {
    const fn ordinary() -> Self {
        Self::Ordinary
    }
    const fn not_between() -> Self {
        Self::NotBetween
    }
    const fn between() -> Self {
        Self::Between
    }
    const fn not_in() -> Self {
        Self::NotIn
    }
    const fn in_list() -> Self {
        Self::In
    }
    const fn not_like() -> Self {
        Self::NotLike
    }
    const fn like() -> Self {
        Self::Like
    }
}

impl SubqueryKind {
    const fn scalar() -> Self {
        Self::Scalar
    }
    const fn exists() -> Self {
        Self::Exists
    }
    const fn any() -> Self {
        Self::Any
    }
}

// DROP TRIGGER retains only syntax; SQLite has no ON relation or drop behavior.
#[derive(Clone, Debug)]
pub(crate) struct DropTrigger {
    name: Name,
    relation: Option<Name>,
    if_exists: bool,
    behavior: Option<TriggerDropBehavior>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TriggerDropBehavior {
    Cascade,
    Restrict,
}

// Unresolved trigger syntax; interpretation belongs to statement-specific analyzers.
#[derive(Clone, Debug)]
pub(crate) struct CreateTrigger {
    name: Name,
    relation: Relation,
    replace: bool,
    if_not_exists: bool,
    temporary: bool,
    timing: Option<TriggerTiming>,
    events: Vec<TriggerEvent>,
    granularity: Option<TriggerGranularity>,
    predicate: Option<Expr>,
    constraint: Option<TriggerConstraint>,
    transitions: Vec<TriggerTransition>,
    body: TriggerBody,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TriggerTiming {
    Before,
    After,
    InsteadOf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TriggerGranularity {
    Row,
    Statement,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum TriggerEvent {
    Insert,
    Update(Vec<String>),
    Delete,
    Truncate,
}

#[derive(Clone, Debug)]
struct TriggerConstraint {
    relation: Option<Relation>,
    deferrable: bool,
    initially_deferred: bool,
}

#[derive(Clone, Debug)]
struct TriggerTransition {
    name: String,
    new: bool,
    table: bool,
}

#[derive(Clone, Debug)]
enum TriggerBody {
    Function(TriggerFunction),
    Commands(Vec<StatementNode>),
}

#[derive(Clone, Debug)]
struct TriggerFunction {
    name: Name,
    arguments: Vec<String>,
}
