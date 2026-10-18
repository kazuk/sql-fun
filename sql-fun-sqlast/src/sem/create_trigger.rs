use crate::sem::traversal::SemanticVisitor;
use crate::sem::{
    AnalysisError, BaseParseContext, ColumnDefinition, ColumnName, FullName, ParseContext, SemAst,
    SemScalarExpr, TypeReference,
};

/// Exact trigger lookup identity, including PostgreSQL's owning relation namespace.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TriggerIdentity {
    name: FullName,
    relation: Option<FullName>,
}
impl TriggerIdentity {
    /// A relation-scoped PostgreSQL trigger.
    pub fn postgres(relation: &FullName, name: &FullName) -> Self {
        Self {
            name: name.clone(),
            relation: Some(relation.clone()),
        }
    }
    /// A schema-scoped SQLite trigger (ASCII case insensitive).
    pub fn sqlite(name: &FullName) -> Self {
        let schema = name.schema().map(Self::sqlite_schema);
        let local = crate::sem::LocalName::from(name.name().as_str().to_ascii_lowercase().as_str());
        Self {
            name: FullName::new(&name.catalog().cloned(), &schema, &local),
            relation: None,
        }
    }
    fn sqlite_schema(schema: &crate::sem::SchemaName) -> crate::sem::SchemaName {
        crate::sem::SchemaName::from(schema.as_str().to_ascii_lowercase().as_str())
    }
    /// Qualified object name.
    pub const fn name(&self) -> &FullName {
        &self.name
    }
    /// PostgreSQL owning relation; absent for SQLite's schema namespace.
    pub const fn relation(&self) -> Option<&FullName> {
        self.relation.as_ref()
    }
}

/// Names of triggers attached to one relation; definitions live only in the context.
#[derive(Debug, Clone, Default)]
pub struct TriggerReferences(Vec<FullName>);
impl TriggerReferences {
    /// Stored fully qualified trigger names.
    pub fn names(&self) -> &[FullName] {
        &self.0
    }
    pub(crate) fn associate(&mut self, name: &FullName) {
        if !self.0.contains(name) {
            self.0.push(name.clone());
        }
    }
    /// Resolve each reference through the effective context, using its dialect namespace.
    pub fn resolve<'a>(
        &self,
        context: &'a impl BaseParseContext,
        target: &FullName,
    ) -> Vec<&'a CreateTrigger> {
        let sqlite = crate::ast::Dialect::from(context.metadata()) == crate::ast::Dialect::sqlite();
        let mut definitions = Vec::new();
        for name in &self.0 {
            let identity = if sqlite {
                TriggerIdentity::sqlite(name)
            } else {
                TriggerIdentity::postgres(target, name)
            };
            if let Some(definition) = context.trigger(&identity) {
                definitions.push(definition);
            }
        }
        definitions
    }
}

/// Trigger firing time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerTiming {
    /// Before the triggering operation.
    Before,
    /// After the triggering operation.
    After,
    /// Replaces the operation on a view.
    InsteadOf,
}
impl TriggerTiming {
    /// BEFORE firing time.
    pub const fn before() -> Self {
        Self::Before
    }
    /// AFTER firing time.
    pub const fn after() -> Self {
        Self::After
    }
    /// INSTEAD OF firing time.
    pub const fn instead_of() -> Self {
        Self::InsteadOf
    }
}
/// Trigger invocation granularity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerGranularity {
    /// Per affected row.
    Row,
    /// Per statement.
    Statement,
}
impl TriggerGranularity {
    /// Row granularity.
    pub const fn row() -> Self {
        Self::Row
    }
    /// Statement granularity.
    pub const fn statement() -> Self {
        Self::Statement
    }
}
/// An event column and its completed lookup outcome.
#[derive(Debug, Clone)]
pub struct TriggerEventColumn {
    name: ColumnName,
    column_type: Option<TypeReference>,
    found: bool,
}
impl TriggerEventColumn {
    pub(crate) fn new(name: &ColumnName, definition: Option<&ColumnDefinition>) -> Self {
        Self {
            name: name.clone(),
            column_type: if let Some(column) = definition {
                column.get_type_cloned()
            } else {
                None
            },
            found: definition.is_some(),
        }
    }
    /// Supplied event column name.
    pub const fn name(&self) -> &ColumnName {
        &self.name
    }
    /// Resolved event-column type.
    pub const fn column_type(&self) -> Option<&TypeReference> {
        self.column_type.as_ref()
    }
    /// Whether column lookup succeeded; SQLite accepts a missing UPDATE OF column.
    pub const fn found(&self) -> bool {
        self.found
    }
}
/// Trigger event with resolved UPDATE OF columns.
#[derive(Debug, Clone)]
pub enum TriggerEvent {
    /// INSERT event.
    Insert,
    /// UPDATE event and its optional column restriction.
    Update(Vec<TriggerEventColumn>),
    /// DELETE event.
    Delete,
    /// PostgreSQL TRUNCATE event.
    Truncate,
}
impl TriggerEvent {
    /// INSERT event.
    pub const fn insert() -> Self {
        Self::Insert
    }
    /// UPDATE event.
    pub fn update(columns: &[TriggerEventColumn]) -> Self {
        Self::Update(columns.to_vec())
    }
    /// DELETE event.
    pub const fn delete() -> Self {
        Self::Delete
    }
    /// TRUNCATE event.
    pub const fn truncate() -> Self {
        Self::Truncate
    }
    /// Event's column restrictions; empty for other events.
    pub fn columns(&self) -> &[TriggerEventColumn] {
        if let Self::Update(columns) = self {
            columns
        } else {
            &[]
        }
    }
}
/// Resolved PostgreSQL trigger function and protocol arguments.
#[derive(Debug, Clone)]
pub struct TriggerFunction {
    name: FullName,
    arguments: Vec<String>,
    return_type: TypeReference,
}
impl TriggerFunction {
    pub(crate) fn new(name: &FullName, arguments: &[String], return_type: &TypeReference) -> Self {
        Self {
            name: name.clone(),
            arguments: arguments.to_vec(),
            return_type: return_type.clone(),
        }
    }
    /// Exact function identity (its SQL signature has zero input parameters).
    pub const fn name(&self) -> &FullName {
        &self.name
    }
    /// Arguments passed as strings through the trigger protocol.
    pub fn arguments(&self) -> &[String] {
        &self.arguments
    }
    /// Verified trigger return type.
    pub const fn return_type(&self) -> &TypeReference {
        &self.return_type
    }
}
/// Semantic trigger body without executing or inlining functions.
#[derive(Debug, Clone)]
pub enum TriggerBody {
    /// PostgreSQL trigger function reference.
    Function(TriggerFunction),
    /// SQLite commands, in source order.
    Commands(Vec<SemAst>),
}
impl TriggerBody {
    /// A function body reference.
    pub fn function(function: &TriggerFunction) -> Self {
        Self::Function(function.clone())
    }
    /// An analyzed command list.
    pub fn commands(commands: &[SemAst]) -> Self {
        Self::Commands(commands.to_vec())
    }
    /// Referenced trigger function.
    pub const fn function_reference(&self) -> Option<&TriggerFunction> {
        if let Self::Function(function) = self {
            Some(function)
        } else {
            None
        }
    }
    /// Analyzed commands, empty for PostgreSQL functions.
    pub fn statements(&self) -> &[SemAst] {
        if let Self::Commands(commands) = self {
            commands
        } else {
            &[]
        }
    }
}
/// Constraint-trigger options and resolved referenced relation.
#[derive(Debug, Clone)]
pub struct TriggerConstraint {
    relation: Option<FullName>,
    deferrable: bool,
    initially_deferred: bool,
}
impl TriggerConstraint {
    pub(crate) fn new(
        relation: &Option<FullName>,
        deferrable: bool,
        initially_deferred: bool,
    ) -> Self {
        Self {
            relation: relation.clone(),
            deferrable,
            initially_deferred,
        }
    }
    /// Referenced relation identity.
    pub const fn relation(&self) -> Option<&FullName> {
        self.relation.as_ref()
    }
    /// Whether firing can be deferred.
    pub const fn deferrable(&self) -> bool {
        self.deferrable
    }
    /// Initial deferred state.
    pub const fn initially_deferred(&self) -> bool {
        self.initially_deferred
    }
}
/// A transition-table binding and its resolved row source.
#[derive(Debug, Clone)]
pub struct TriggerTransition {
    name: String,
    new: bool,
    relation: FullName,
}
impl TriggerTransition {
    pub(crate) fn new(name: &str, new: bool, relation: &FullName) -> Self {
        Self {
            name: name.to_owned(),
            new,
            relation: relation.clone(),
        }
    }
    /// Local transition-table alias.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Whether this binding exposes after-images.
    pub const fn is_new(&self) -> bool {
        self.new
    }
    /// Relation supplying the transition row type.
    pub const fn relation(&self) -> &FullName {
        &self.relation
    }
}
/// Completed CREATE TRIGGER semantics. Applying it is a separate catalog operation.
#[derive(Debug, Clone)]
pub struct CreateTrigger {
    identity: TriggerIdentity,
    target: FullName,
    timing: TriggerTiming,
    granularity: TriggerGranularity,
    events: Vec<TriggerEvent>,
    predicate: Option<SemScalarExpr>,
    constraint: Option<TriggerConstraint>,
    transitions: Vec<TriggerTransition>,
    body: TriggerBody,
    replace: bool,
    if_not_exists: bool,
    temporary: bool,
    dependencies: TriggerDependencies,
    source: Option<crate::sem::SourcePathWithSpan>,
}
impl CreateTrigger {
    pub(crate) fn new(
        identity: &TriggerIdentity,
        target: &FullName,
        timing: TriggerTiming,
        granularity: TriggerGranularity,
        body: &TriggerBody,
    ) -> Self {
        Self {
            identity: identity.clone(),
            target: target.clone(),
            timing,
            granularity,
            events: Vec::new(),
            predicate: None,
            constraint: None,
            transitions: Vec::new(),
            body: body.clone(),
            replace: false,
            if_not_exists: false,
            temporary: false,
            dependencies: TriggerDependencies::default(),
            source: None,
        }
    }
    pub(crate) fn set_event_semantics(
        &mut self,
        events: &[TriggerEvent],
        predicate: &Option<SemScalarExpr>,
        constraint: &Option<TriggerConstraint>,
        transitions: &[TriggerTransition],
    ) {
        self.events = events.to_vec();
        self.predicate = predicate.clone();
        self.constraint = constraint.clone();
        self.transitions = transitions.to_vec();
    }
    pub(crate) const fn set_creation_mode(
        &mut self,
        replace: bool,
        if_not_exists: bool,
        temporary: bool,
    ) {
        self.replace = replace;
        self.if_not_exists = if_not_exists;
        self.temporary = temporary;
    }
    pub(crate) fn column(
        context: &impl ParseContext,
        target: &FullName,
        column: &ColumnName,
    ) -> Option<ColumnDefinition> {
        let sqlite = context.dialect() == crate::ast::Dialect::sqlite();
        if let Some(table) = context.get_table_impl(target) {
            let mut index = 0;
            while let Some(definition) = table.get_column_at(index) {
                if definition.name().as_ref().is_some_and(|name| {
                    name == column
                        || (sqlite && name.as_str().eq_ignore_ascii_case(column.as_str()))
                }) {
                    return Some(definition.clone());
                }
                index += 1;
            }
        } else if let Some(view) = context.get_view_impl(target) {
            let columns = view.result_columns();
            let mut index = 0;
            while let Some(result) = columns.get_column_at(index) {
                let definition = result.create_column_definition();
                if definition.name().as_ref().is_some_and(|name| {
                    name == column
                        || (sqlite && name.as_str().eq_ignore_ascii_case(column.as_str()))
                }) {
                    return Some(definition);
                }
                index += 1;
            }
        }
        None
    }
    pub(crate) fn retain_dependencies(
        &mut self,
        context: &impl ParseContext,
    ) -> Result<(), AnalysisError> {
        let mut dependencies = TriggerDependencies::default();
        dependencies.relation_identity(&self.target);
        if let Some(constraint) = &self.constraint
            && let Some(relation) = constraint.relation()
        {
            dependencies.relation_identity(relation);
        }
        if let Some(predicate) = &mut self.predicate {
            predicate.walk(&mut dependencies)?;
        }
        self.body.collect_dependencies(&mut dependencies)?;
        self.dependencies = dependencies;
        self.source = context.get_source_path_with_span().cloned();
        Ok(())
    }
    /// Resolved dependencies retained without re-running lookup.
    pub const fn dependencies(&self) -> &TriggerDependencies {
        &self.dependencies
    }
    /// Source statement location, when supplied by the caller.
    pub const fn source(&self) -> Option<&crate::sem::SourcePathWithSpan> {
        self.source.as_ref()
    }
    /// Exact named registration identity.
    pub const fn identity(&self) -> &TriggerIdentity {
        &self.identity
    }
    /// Resolved target relation.
    pub const fn target(&self) -> &FullName {
        &self.target
    }
    /// Trigger firing time.
    pub const fn timing(&self) -> TriggerTiming {
        self.timing
    }
    /// Trigger granularity.
    pub const fn granularity(&self) -> TriggerGranularity {
        self.granularity
    }
    /// Resolved events.
    pub fn events(&self) -> &[TriggerEvent] {
        &self.events
    }
    /// Analyzed WHEN expression.
    pub const fn predicate(&self) -> Option<&SemScalarExpr> {
        self.predicate.as_ref()
    }
    /// Constraint options.
    pub const fn constraint(&self) -> Option<&TriggerConstraint> {
        self.constraint.as_ref()
    }
    /// Transition table bindings.
    pub fn transitions(&self) -> &[TriggerTransition] {
        &self.transitions
    }
    /// Analyzed function or command body.
    pub const fn body(&self) -> &TriggerBody {
        &self.body
    }
    /// Whether this statement requests replacement.
    pub const fn replaces(&self) -> bool {
        self.replace
    }
    /// Whether an existing identity is retained.
    pub const fn if_not_exists(&self) -> bool {
        self.if_not_exists
    }
    /// Whether this is a SQLite TEMP trigger.
    pub const fn temporary(&self) -> bool {
        self.temporary
    }
    pub(crate) fn apply<C: ParseContext>(&self, context: C) -> Result<C, AnalysisError> {
        if let Some(existing) = context.trigger(self.identity()) {
            if self.if_not_exists {
                return Ok(context);
            }
            if !self.replace || existing.constraint().is_some() || self.constraint.is_some() {
                return Err(AnalysisError::trigger(
                    &crate::sem::problem::TriggerProblem::duplicate(self.identity()),
                ));
            }
        }
        // DDL determines the association; storage only performs the supplied update.
        let context = context.associate_trigger(&self.target, self.identity.name())?;
        context.register_trigger(self)
    }
}

/// Catalog dependencies retained by a resolved trigger definition.
#[derive(Debug, Clone, Default)]
pub struct TriggerDependencies {
    relations: Vec<FullName>,
    functions: Vec<FullName>,
    types: Vec<FullName>,
}
impl TriggerDependencies {
    /// Resolved relation dependencies.
    pub fn relations(&self) -> &[FullName] {
        &self.relations
    }
    /// Resolved function dependencies, including the trigger function.
    pub fn functions(&self) -> &[FullName] {
        &self.functions
    }
    /// Resolved expression type dependencies.
    pub fn types(&self) -> &[FullName] {
        &self.types
    }
    fn add_function(&mut self, name: &FullName) {
        if !self.functions.contains(name) {
            self.functions.push(name.clone());
        }
    }
}
impl crate::sem::traversal::SemanticVisitor for TriggerDependencies {
    fn relation_identity(&mut self, name: &FullName) {
        if !self.relations.contains(name) {
            self.relations.push(name.clone());
        }
    }
    fn expression(&mut self, expression: &mut SemScalarExpr) -> Result<(), AnalysisError> {
        if let Some(name) = expression.function_dependency() {
            self.add_function(name);
        }
        if let Some(ty) = expression.get_type()
            && !self.types.contains(ty.full_name())
        {
            self.types.push(ty.full_name().clone());
        }
        Ok(())
    }
}
impl TriggerBody {
    fn collect_dependencies(
        &mut self,
        dependencies: &mut TriggerDependencies,
    ) -> Result<(), AnalysisError> {
        match self {
            Self::Function(function) => {
                dependencies.add_function(function.name());
            }
            Self::Commands(commands) => {
                for command in commands {
                    command.walk_trigger_command(dependencies)?;
                }
            }
        }
        Ok(())
    }
}
