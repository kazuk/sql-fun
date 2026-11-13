//! Owned DDL syntax. Catalog bindings are obtained only during resolution.

mod postgres_helpers;

#[path = "deparse/ddl.rs"]
mod sql_emission;

use crate::{
    StringSpan,
    ast::{
        AlterCommand, AlterRelationKind, Call, CallKind, Column as AstColumn, ColumnConstraint,
        CreateTableMode, Expr, IndexExpression, Into as AstInto, Literal, Name, PartitionBound,
        Query, Relation as AstRelation, Role as AstRole, StatementNode, SyntaxOption,
        TableConstraint, TypeName, VariableSet,
    },
    error::AnalysisError,
    names::FullName,
    syn,
};

impl From<&FullName> for Name {
    fn from(value: &FullName) -> Self {
        let mut parts = Vec::new();
        if let Some(catalog) = value.catalog() {
            parts.push(catalog.to_string());
        }
        if let Some(schema) = value.schema() {
            parts.push(schema.to_string());
        }
        parts.push(value.name().to_string());
        Self::from_parts(&parts)
    }
}

impl super::AlterRelation {
    pub(crate) fn single_column_addition(table: &FullName, column: &AstColumn) -> Self {
        Self {
            name: Name::from(table),
            kind: AlterRelationKind::table(),
            commands: vec![AlterCommand::AddColumn(column.clone())],
            if_exists: false,
            only: false,
        }
    }

    pub(crate) const fn with_view_kind(mut self, materialized: bool) -> Self {
        self.kind = AlterRelationKind::view(materialized);
        self
    }
}

impl super::AlterRelationKind {
    pub(crate) const fn table() -> Self {
        Self::Table
    }

    pub(crate) const fn view(materialized: bool) -> Self {
        if materialized {
            Self::MaterializedView
        } else {
            Self::View
        }
    }

    pub(crate) const fn supports_only(self) -> bool {
        matches!(self, Self::Table | Self::ForeignTable)
    }
}

#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct PostgresAlterCommand {
    #[getter(copy)]
    operation: AlterOperation,
    name: String,
    #[getter(copy)]
    number: i32,
    definition: AlterDefinition,
    #[getter(copy)]
    missing_ok: bool,
    #[getter(copy)]
    cascade: bool,
    legacy: Option<Box<AlterCommand>>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum AlterOperation {
    AddColumn,
    ColumnDefault,
    DropNotNull,
    SetNotNull,
    SetExpression,
    DropExpression,
    SetStatistics,
    SetOptions,
    ResetOptions,
    SetStorage,
    SetCompression,
    DropColumn,
    AddConstraint,
    AlterConstraint,
    ValidateConstraint,
    DropConstraint,
    AlterColumnType,
    AlterColumnGenericOptions,
    Owner,
    Cluster,
    DropCluster,
    SetLogged,
    SetUnlogged,
    DropOids,
    SetAccessMethod,
    SetTablespace,
    SetRelOptions,
    ResetRelOptions,
    EnableTrigger,
    EnableAlwaysTrigger,
    EnableReplicaTrigger,
    DisableTrigger,
    EnableTriggerAll,
    DisableTriggerAll,
    EnableTriggerUser,
    DisableTriggerUser,
    EnableRule,
    EnableAlwaysRule,
    EnableReplicaRule,
    DisableRule,
    AddInherit,
    DropInherit,
    AddOf,
    DropOf,
    ReplicaIdentity,
    EnableRowSecurity,
    DisableRowSecurity,
    ForceRowSecurity,
    NoForceRowSecurity,
    GenericOptions,
    AttachPartition,
    DetachPartition,
    DetachPartitionFinalize,
    AddIdentity,
    SetIdentity,
    DropIdentity,
}

#[derive(Clone, Debug)]
pub(crate) enum AlterDefinition {
    None,
    Expression(Expr),
    Text(String),
    Options(Vec<SyntaxOption>),
    GenericOptions(Vec<GenericOption>),
    Column(AstColumn),
    ColumnType(AlterColumnType),
    Constraint(TableConstraint),
    ConstraintFlags(AlterConstraintFlags),
    Role(AstRole),
    Relation(Name),
    Type(TypeName),
    Replica(ReplicaIdentity),
    Partition(AlterPartition),
    Identity(AlterIdentity),
}

impl AlterDefinition {
    pub(crate) const fn as_column(&self) -> Option<&AstColumn> {
        if let Self::Column(column) = self {
            Some(column)
        } else {
            None
        }
    }

    pub(crate) const fn none() -> Self {
        Self::None
    }

    pub(crate) const fn expression(value: Expr) -> Self {
        Self::Expression(value)
    }

    pub(crate) fn text(value: &str) -> Self {
        Self::Text(value.to_owned())
    }

    pub(crate) const fn options(values: Vec<SyntaxOption>) -> Self {
        Self::Options(values)
    }

    pub(crate) const fn generic_options(values: Vec<GenericOption>) -> Self {
        Self::GenericOptions(values)
    }

    pub(crate) const fn column(value: AstColumn) -> Self {
        Self::Column(value)
    }

    pub(crate) const fn column_type(value: AlterColumnType) -> Self {
        Self::ColumnType(value)
    }

    pub(crate) const fn constraint(value: TableConstraint) -> Self {
        Self::Constraint(value)
    }

    pub(crate) const fn constraint_flags(value: AlterConstraintFlags) -> Self {
        Self::ConstraintFlags(value)
    }

    pub(crate) const fn role(value: AstRole) -> Self {
        Self::Role(value)
    }

    pub(crate) const fn relation(value: Name) -> Self {
        Self::Relation(value)
    }

    pub(crate) const fn type_name(value: TypeName) -> Self {
        Self::Type(value)
    }

    pub(crate) const fn replica(value: ReplicaIdentity) -> Self {
        Self::Replica(value)
    }

    pub(crate) const fn partition(value: AlterPartition) -> Self {
        Self::Partition(value)
    }

    pub(crate) const fn identity(value: AlterIdentity) -> Self {
        Self::Identity(value)
    }
}

#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct AlterColumnType {
    type_name: TypeName,
    collation: Option<Name>,
    using: Option<Expr>,
}
#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct AlterConstraintFlags {
    #[getter(skip)]
    name: String,
    #[getter(skip)]
    deferrability: Option<(bool, bool)>,
    #[getter(copy)]
    enforced: Option<bool>,
    #[getter(copy)]
    no_inherit: Option<bool>,
}
impl TryFrom<&syn::AtAlterConstraint> for AlterConstraintFlags {
    type Error = AnalysisError;
    fn try_from(value: &syn::AtAlterConstraint) -> Result<Self, Self::Error> {
        Ok(Self {
            name: value.get_conname(),
            deferrability: if value.get_alter_deferrability() {
                Some((value.get_deferrable(), value.get_initdeferred()))
            } else {
                None
            },
            enforced: if value.get_alter_enforceability() {
                Some(value.get_is_enforced())
            } else {
                None
            },
            no_inherit: if value.get_alter_inheritability() {
                Some(value.get_noinherit())
            } else {
                None
            },
        })
    }
}

#[derive(Clone, Debug)]
pub(crate) struct GenericOption {
    action: GenericAction,
    name: String,
    value: Option<String>,
}
#[derive(Clone, Copy, Debug)]
pub(crate) enum GenericAction {
    Unspecified,
    Add,
    Set,
    Drop,
}
#[derive(Clone, Debug)]
pub(crate) struct ReplicaIdentity {
    kind: String,
    index: String,
}
#[derive(Clone, Debug)]
pub(crate) struct AlterPartition {
    relation: Name,
    bound: Option<PartitionBound>,
    concurrent: bool,
}
#[derive(Clone, Debug)]
pub(crate) struct AlterIdentity {
    mode: String,
    options: Vec<SyntaxOption>,
}

impl PostgresAlterCommand {
    pub(crate) fn new(
        operation: AlterOperation,
        name: &str,
        number: i32,
        definition: &AlterDefinition,
        missing_ok: bool,
        cascade: bool,
        legacy: Option<&AlterCommand>,
    ) -> Self {
        Self {
            operation,
            name: name.to_owned(),
            number,
            definition: definition.clone(),
            missing_ok,
            cascade,
            legacy: legacy.cloned().map(Box::new),
        }
    }
}

impl AlterColumnType {
    pub(crate) fn new(
        type_name: &TypeName,
        collation: Option<&Name>,
        using: Option<&Expr>,
    ) -> Self {
        Self {
            type_name: type_name.clone(),
            collation: collation.cloned(),
            using: using.cloned(),
        }
    }
}
impl GenericOption {
    pub(crate) fn new(action: GenericAction, name: &str, value: Option<&str>) -> Self {
        Self {
            action,
            name: name.to_owned(),
            value: value.map(str::to_owned),
        }
    }
}
impl ReplicaIdentity {
    pub(crate) fn new(kind: &str, index: &str) -> Self {
        Self {
            kind: kind.to_owned(),
            index: index.to_owned(),
        }
    }
}
impl AlterPartition {
    pub(crate) fn new(relation: &Name, bound: Option<&PartitionBound>, concurrent: bool) -> Self {
        Self {
            relation: relation.clone(),
            bound: bound.cloned(),
            concurrent,
        }
    }
}
impl AlterIdentity {
    pub(crate) fn new(mode: &str, options: &[SyntaxOption]) -> Self {
        Self {
            mode: mode.to_owned(),
            options: options.to_vec(),
        }
    }
}

impl super::AlterCommand {
    pub(crate) fn postgres(command: PostgresAlterCommand) -> Self {
        Self::Postgres(Box::new(command))
    }
}

impl super::ColumnConstraint {
    pub(crate) const fn is_unique_key(&self) -> bool {
        matches!(self, Self::PrimaryKey(_, _) | Self::Unique(_))
    }
}

impl super::Table {
    pub(crate) fn named(mut self, name: &FullName, mode: CreateTableMode, temporary: bool) -> Self {
        self.name = Name::from(name);
        self.mode = mode;
        self.temporary = temporary;
        self
    }
}

#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct TableAs {
    into: AstInto,
    query: Query,
    #[getter(copy)]
    materialized: bool,
    #[getter(copy)]
    if_not_exists: bool,
    #[getter(skip)]
    skip_data: bool,
}

impl TableAs {
    pub(crate) const fn new(
        into: AstInto,
        query: Query,
        materialized: bool,
        if_not_exists: bool,
    ) -> Self {
        Self {
            into,
            query,
            materialized,
            if_not_exists,
            skip_data: false,
        }
    }

    pub(crate) const fn with_skip_data(mut self, skip_data: bool) -> Self {
        self.skip_data = skip_data;
        self
    }
}

#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct Index {
    name: String,
    relation: AstRelation,
    keys: Vec<IndexExpression>,
    includes: Vec<IndexExpression>,
    #[getter(copy)]
    unique: bool,
    predicate: Option<Expr>,
    options: Vec<SyntaxOption>,
    access_method: String,
    #[getter(skip)]
    concurrent: bool,
    #[getter(copy)]
    if_not_exists: bool,
    #[getter(skip)]
    nulls_not_distinct: bool,
    #[getter(skip)]
    tablespace: String,
}

impl Index {
    #[expect(clippy::too_many_arguments)]
    pub(crate) fn new(
        name: &str,
        relation: AstRelation,
        keys: Vec<IndexExpression>,
        includes: Vec<IndexExpression>,
        unique: bool,
        predicate: Option<Expr>,
        options: Vec<SyntaxOption>,
        access_method: &str,
    ) -> Self {
        Self {
            name: name.to_owned(),
            relation,
            keys,
            includes,
            unique,
            predicate,
            options,
            access_method: access_method.to_owned(),
            concurrent: false,
            if_not_exists: false,
            nulls_not_distinct: false,
            tablespace: String::new(),
        }
    }

    pub(crate) fn with_syntax_options(
        mut self,
        concurrent: bool,
        if_not_exists: bool,
        nulls_not_distinct: bool,
        tablespace: &str,
    ) -> Self {
        self.concurrent = concurrent;
        self.if_not_exists = if_not_exists;
        self.nulls_not_distinct = nulls_not_distinct;
        self.tablespace = tablespace.to_owned();
        self
    }
}

#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct Schema {
    name: String,
    elements: Vec<StatementNode>,
    #[getter(copy)]
    if_not_exists: bool,
    authorization: Option<AstRole>,
}

impl Schema {
    pub(crate) fn new(name: &str, elements: Vec<StatementNode>, if_not_exists: bool) -> Self {
        Self {
            name: name.to_owned(),
            elements,
            if_not_exists,
            authorization: None,
        }
    }

    pub(crate) fn with_authorization(mut self, authorization: Option<AstRole>) -> Self {
        self.authorization = authorization;
        self
    }
}

#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct Function {
    name: Name,
    parameters: Vec<FunctionParameter>,
    result_type: Option<TypeName>,
    options: Vec<FunctionOption>,
    #[getter(copy)]
    replace: bool,
    #[getter(copy)]
    procedure: bool,
    #[getter(skip)]
    body: Option<FunctionBody>,
}

#[derive(Clone, Debug)]
pub(crate) enum FunctionOption {
    Body(Vec<String>),
    Language(String),
    Transform(Vec<TypeName>),
    Window,
    Volatility(String),
    Leakproof(bool),
    Strict(bool),
    SecurityDefiner(bool),
    Parallel(String),
    Cost(Expr),
    Rows(Expr),
    Support(Name),
    Setting(VariableSet),
}

impl FunctionOption {
    pub(crate) const fn is_strict(&self) -> bool {
        matches!(self, Self::Strict(true))
    }
}

#[derive(Clone, Debug)]
pub(crate) enum FunctionBody {
    Return(Expr),
    Atomic(Vec<FunctionBodyStatement>),
}

#[derive(Clone, Debug)]
pub(crate) enum FunctionBodyStatement {
    Return(Expr),
    Statement(Box<StatementNode>),
}

impl FunctionBodyStatement {
    pub(crate) const fn return_expression(expression: Expr) -> Self {
        Self::Return(expression)
    }

    pub(crate) fn statement(statement: StatementNode) -> Self {
        Self::Statement(Box::new(statement))
    }
}

#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct FunctionParameter {
    name: String,
    type_name: TypeName,
    #[getter(copy)]
    mode: ParameterMode,
    default: Option<Expr>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum ParameterMode {
    In,
    Out,
    InOut,
    Variadic,
    Table,
}

impl FunctionParameter {
    pub(crate) fn new(
        name: &str,
        type_name: TypeName,
        mode: ParameterMode,
        default: Option<Expr>,
    ) -> Self {
        Self {
            name: name.to_owned(),
            type_name,
            mode,
            default,
        }
    }
}

impl ParameterMode {
    pub(crate) const fn is_output(self) -> bool {
        matches!(self, Self::Out | Self::InOut | Self::Table)
    }
}

impl Function {
    pub(crate) const fn new(
        name: Name,
        parameters: Vec<FunctionParameter>,
        result_type: Option<TypeName>,
        options: Vec<FunctionOption>,
        replace: bool,
        procedure: bool,
    ) -> Self {
        Self {
            name,
            parameters,
            result_type,
            options,
            replace,
            procedure,
            body: None,
        }
    }

    pub(crate) fn with_body(mut self, body: Option<FunctionBody>) -> Self {
        self.body = body;
        self
    }
}

#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct Domain {
    name: Name,
    base: TypeName,
    base_span: StringSpan,
    constraints: Vec<TableConstraint>,
    collation: Name,
}

#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct CompositeType {
    name: Name,
    fields: Vec<CompositeField>,
}

#[derive(Clone, Debug, derive_getters::Getters)]
pub(crate) struct CompositeField {
    name: String,
    type_name: TypeName,
    collation: Option<Name>,
    #[getter(copy)]
    is_not_null: bool,
}

impl CompositeType {
    pub(crate) fn new(name: &Name, fields: &[CompositeField]) -> Self {
        Self {
            name: name.clone(),
            fields: fields.to_vec(),
        }
    }
}

impl CompositeField {
    pub(crate) fn new(
        name: &str,
        type_name: &TypeName,
        collation: Option<&Name>,
        is_not_null: bool,
    ) -> Self {
        Self {
            name: name.to_owned(),
            type_name: type_name.clone(),
            collation: collation.cloned(),
            is_not_null,
        }
    }
}

impl Domain {
    pub(crate) const fn new(
        name: Name,
        base: TypeName,
        base_span: StringSpan,
        constraints: Vec<TableConstraint>,
        collation: Name,
    ) -> Self {
        Self {
            name,
            base,
            base_span,
            constraints,
            collation,
        }
    }
}

impl TableConstraint {
    pub(crate) fn unnamed_check(constraint: &ColumnConstraint) -> Self {
        Self {
            extras: Default::default(),
            name: None,
            keys: Vec::new(),
            constraint: constraint.clone(),
            order: Vec::new(),
            auto_increment: false,
        }
    }

    pub(crate) const fn is_check(&self) -> bool {
        matches!(self.constraint, ColumnConstraint::Check(_))
    }
}

impl ColumnConstraint {
    #[cfg(feature = "sqlite")]
    pub(crate) fn check(expression: &Expr) -> Self {
        Self::Check(expression.clone())
    }
}

impl AlterRelationKind {
    pub(crate) const fn supports_row_security(self) -> bool {
        matches!(self, Self::Table)
    }

    pub(crate) const fn supports_table_column_attributes(self) -> bool {
        matches!(self, Self::Table)
    }
}

impl AstColumn {
    /// Expand the selected SERIAL spelling into ordinary integer declaration syntax.
    /// The semantic consumer selects the integer type and stages catalog effects.
    pub(crate) fn serial_expansion(&self, integer: &str) -> Self {
        let mut expanded = self.clone();
        let syntax = expanded
            .type_name
            .as_mut()
            .expect("serial declaration type");
        syntax.name = Name::from_parts(&["pg_catalog".to_owned(), integer.to_owned()]);
        syntax.array_bounds.clear();
        expanded
    }
}

impl ColumnConstraint {
    pub(crate) const fn serial_not_null() -> Self {
        Self::Nullability(true, None, None)
    }
    pub(crate) fn serial_default(sequence: &FullName) -> Self {
        Self::Default(Expr::serial_next_value(sequence))
    }
}

impl Expr {
    pub(crate) fn serial_next_value(sequence: &FullName) -> Self {
        let literal = Self::Literal(Literal::sequence_reference(sequence));
        let target = TypeName::projection(
            &Name::from_parts(&["pg_catalog".to_owned(), "regclass".to_owned()]),
            false,
        );
        let argument = Self::projection_cast(&literal, &target);
        Self::projection_call(&Call::serial_next_value(&argument))
    }
}

impl Literal {
    pub(crate) fn sequence_reference(sequence: &FullName) -> Self {
        Self::String(sequence.to_string())
    }
}

impl Call {
    pub(crate) fn serial_next_value(argument: &Expr) -> Self {
        Self {
            sql_syntax: false,
            kind: CallKind::ordinary(),
            name: Name::from_parts(&["pg_catalog".to_owned(), "nextval".to_owned()]),
            arguments: vec![argument.clone()],
            argument_locations: vec![None],
            star: false,
            distinct: false,
            variadic: false,
            within_group: false,
            within_order: Vec::new(),
            order: Vec::new(),
            filter: None,
            over: None,
            location: None,
        }
    }
}

impl AstColumn {
    /// Analyze REFERENCES after all effective columns and keys are staged.
    pub(crate) fn without_foreign_keys(&self) -> Self {
        let mut column = self.clone();
        column.constraints.retain(|constraint| {
            !matches!(constraint.constraint(), ColumnConstraint::ForeignKey(_))
        });
        column
    }
}
