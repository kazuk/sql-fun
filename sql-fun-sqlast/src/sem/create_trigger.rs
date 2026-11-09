mod ast;

use crate::{
    ast::Dialect,
    context::BaseParseContext,
    names::{ColumnName, FullName, LocalName, SchemaName, TableName},
    sem::{
        AnalysisError, AnalysisProblem, ColumnDefinition, ColumnDrop, ColumnRename, ParseContext,
        SemAst, SemScalarExpr, SourcePathWithSpan, TypeReference,
        problem::TriggerProblem,
        traversal::{ColumnRenamer, RelationRenamer, SemanticVisitorMut, optional},
    },
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
        let local = LocalName::from(name.name().as_str().to_ascii_lowercase().as_str());
        Self {
            name: FullName::new(&name.catalog().cloned(), &schema, &local),
            relation: None,
        }
    }
    fn sqlite_schema(schema: &SchemaName) -> SchemaName {
        SchemaName::from(schema.as_str().to_ascii_lowercase().as_str())
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

impl std::fmt::Display for TriggerIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)?;
        if let Some(relation) = &self.relation {
            write!(f, " on {relation}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod identity_display_tests {
    use crate::{names::FullName, sem::TriggerIdentity};

    #[test]
    fn distinguishes_relation_and_schema_namespaces() {
        let target: FullName = "public.items".parse().unwrap();
        let name: FullName = r#"public."changed.items""#.parse().unwrap();
        assert_eq!(
            TriggerIdentity::postgres(&target, &name).to_string(),
            r#"public."changed.items" on public.items"#
        );
        assert_eq!(
            TriggerIdentity::sqlite(&name).to_string(),
            r#"public."changed.items""#
        );
    }
}

/// Names of triggers attached to one relation; definitions live only in the context.
///
/// Use a table's `triggers()` or an ordinary view's `triggers()` to read the
/// stored names. [`Self::resolve`] resolves them
/// through the same effective context as direct
/// [`crate::context::TriggerInfoRead::trigger`] lookup. Changes in a child schema layer
/// preserve frozen parents and sibling contexts. Registering these definitions
/// does not itself calculate view writability.
///
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
    pub(crate) fn dissociate(&mut self, name: &FullName) {
        self.0.retain(|stored| stored != name);
    }
    /// Resolve each reference through the effective context, using its dialect namespace.
    pub fn resolve<'a>(
        &self,
        context: &'a impl BaseParseContext,
        target: &FullName,
    ) -> Vec<&'a CreateTrigger> {
        let sqlite = Dialect::from(context.metadata()) == Dialect::sqlite();
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
    fn rename_column(&mut self, rename: &ColumnRename) {
        if let Self::Update(columns) = self {
            for column in columns {
                if column.found {
                    rename.rename_name(&mut column.name);
                }
            }
        }
    }

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
    fn rename_relation(
        &mut self,
        visitor: &mut impl SemanticVisitorMut,
    ) -> Result<(), AnalysisError> {
        if let Self::Commands(commands) = self {
            for command in commands {
                command.walk_nested_command(visitor)?;
            }
        }
        Ok(())
    }

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
    source: Option<SourcePathWithSpan>,
}
impl CreateTrigger {
    pub(crate) fn drop_event_depends_on(
        &self,
        context: &impl ParseContext,
        change: &ColumnDrop,
    ) -> bool {
        self.target() == change.table().full_name()
            && self
                .events
                .iter()
                .any(|event| event.drop_depends_on(context, change))
    }
    pub(crate) fn walk_column_dependencies(
        &mut self,
        visitor: &mut impl SemanticVisitorMut,
    ) -> Result<(), AnalysisError> {
        optional(&mut self.predicate, visitor)?;
        self.body.rename_relation(visitor)
    }

    pub(crate) fn rename_column(&mut self, rename: &ColumnRename) -> Result<(), AnalysisError> {
        self.rename_column_with(rename, &mut ColumnRenamer::new(rename))
    }
    pub(crate) fn rename_column_with(
        &mut self,
        rename: &ColumnRename,
        visitor: &mut impl SemanticVisitorMut,
    ) -> Result<(), AnalysisError> {
        if &self.target == rename.table().full_name() {
            for event in &mut self.events {
                event.rename_column(rename);
            }
        }
        self.walk_column_dependencies(visitor)
    }

    pub(crate) fn rename_relation(
        &mut self,
        old: &TableName,
        new: &TableName,
    ) -> Result<(), AnalysisError> {
        if self.identity.relation.as_ref() == Some(old.full_name()) {
            self.identity.relation = Some(new.full_name().clone());
        }
        if &self.target == old.full_name() {
            self.target = new.full_name().clone();
        }
        if let Some(constraint) = &mut self.constraint
            && constraint.relation.as_ref() == Some(old.full_name())
        {
            constraint.relation = Some(new.full_name().clone());
        }
        for transition in &mut self.transitions {
            if &transition.relation == old.full_name() {
                transition.relation = new.full_name().clone();
            }
        }
        let mut visitor = RelationRenamer::new(old, new);
        if let Some(predicate) = &mut self.predicate {
            predicate.walk(&mut visitor)?;
        }
        self.body.rename_relation(&mut visitor)?;
        for name in &mut self.dependencies.relations {
            if name == old.full_name() {
                *name = new.full_name().clone();
            }
        }
        for name in &mut self.dependencies.types {
            if name == old.full_name() {
                *name = new.full_name().clone();
            }
        }
        Ok(())
    }

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
        let sqlite = context.dialect() == Dialect::sqlite();
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
}

#[cfg(test)]
mod test_retains_resolved_old_new_bindings_and_body_dependencies {
    #[cfg(feature = "sqlite")]
    use crate::{ast::Dialect, names::FullName, test_helpers::trigger as test_helpers};

    #[cfg(feature = "sqlite")]
    #[test]
    fn retains_resolved_old_new_bindings_and_body_dependencies() -> testresult::TestResult {
        let mut context = test_helpers::catalog(Dialect::sqlite());
        let statement = test_helpers::resolve(
            "CREATE TRIGGER audit AFTER UPDATE ON items BEGIN SELECT OLD.id, NEW.id; INSERT INTO other(id) VALUES(NEW.id); END",
            &mut context,
        )?;
        let trigger = statement.trigger().unwrap();
        let commands = trigger.body().statements();
        let query = commands[0].clone().into_select_statemtnt().unwrap();
        for (index, alias) in ["old", "new"].into_iter().enumerate() {
            let expression = query
                .result_columns()
                .get_column_at(index)
                .unwrap()
                .expression()
                .unwrap();
            let binding = expression.table_column_reference().unwrap();
            assert_eq!(binding.relation(), &FullName::with_schema("main", "items"));
            assert_eq!(binding.alias().as_str(), alias);
            assert_eq!(
                expression.get_type(),
                Some(crate::sem::PgBuiltInType::int4())
            );
        }
        assert!(
            trigger
                .dependencies()
                .relations()
                .contains(&FullName::with_schema("main", "other"))
        );
        assert!(
            trigger.dependencies().types().contains(
                crate::sem::PgBuiltInType::int4()
                    .full_name()
                    .expect("builtin identity")
            )
        );
        Ok(())
    }
}

impl CreateTrigger {
    /// Resolved dependencies retained without re-running lookup.
    pub const fn dependencies(&self) -> &TriggerDependencies {
        &self.dependencies
    }
    /// Source statement location, when supplied by the caller.
    pub const fn source(&self) -> Option<&SourcePathWithSpan> {
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
    pub(crate) fn apply<C: ParseContext>(&self, mut context: C) -> Result<C, AnalysisError> {
        if let Some(existing) = context.trigger(self.identity()) {
            if self.if_not_exists {
                return Ok(context);
            }
            if !self.replace || existing.constraint().is_some() || self.constraint.is_some() {
                context.report_analysis_problem(AnalysisProblem::trigger(
                    &TriggerProblem::duplicate(self.identity()),
                ))?;
                return Ok(context);
            }
        }
        if context.resolve_relation_name(&self.target).is_none() {
            context.report_analysis_problem(AnalysisProblem::trigger(&TriggerProblem::target(
                &self.target,
            )))?;
            return Ok(context);
        }
        // DDL determines the association; storage only performs the supplied update.
        let context = context.associate_trigger(&self.target, self.identity.name())?;
        context.register_trigger(self)
    }
}

#[cfg(test)]
mod test_named_lookup_and_target_links_agree_across_layers {
    #[cfg(feature = "sqlite")]
    use crate::context::ViewInfoRead as ContextViewInfoRead;

    use crate::{
        ast::Dialect,
        context::{ParseContext, TableInfoRead, TriggerInfoRead},
        names::FullName,
        sem::TriggerIdentity,
        test_helpers::trigger as test_helpers,
    };

    #[test]
    fn named_lookup_and_target_links_agree_across_layers() -> testresult::TestResult {
        let root = test_helpers::catalog(Dialect::postgres()).freeze();
        let parent = test_helpers::layer(&root);
        let parent = test_helpers::apply(
            "CREATE TRIGGER audit AFTER UPDATE ON items EXECUTE FUNCTION audit_fn()",
            parent,
        )?
        .into_context();
        let parent = parent.freeze();
        let sibling = test_helpers::layer(&parent);
        let child = test_helpers::apply(
            "CREATE TRIGGER second BEFORE UPDATE ON items EXECUTE FUNCTION audit_fn()",
            test_helpers::layer(&parent),
        )?
        .into_context();
        let target = FullName::with_schema("public", "items");
        let names = child.get_table_impl(&target).unwrap().triggers();
        assert_eq!(names.names().len(), 2);
        let definitions = names.resolve(&child, &target);
        assert_eq!(definitions.len(), 2);
        for definition in definitions {
            assert!(std::ptr::eq(
                definition,
                child.trigger(definition.identity()).unwrap()
            ));
        }
        assert_eq!(
            parent
                .get_table_impl(&target)
                .unwrap()
                .triggers()
                .names()
                .len(),
            1
        );
        assert_eq!(
            sibling
                .get_table_impl(&target)
                .unwrap()
                .triggers()
                .names()
                .len(),
            1
        );
        let child = test_helpers::apply("CREATE OR REPLACE TRIGGER audit BEFORE UPDATE ON items EXECUTE FUNCTION audit_fn('changed')", child)?.into_context();
        assert_eq!(
            child
                .get_table_impl(&target)
                .unwrap()
                .triggers()
                .names()
                .len(),
            2
        );
        let identity =
            TriggerIdentity::postgres(&target, &FullName::with_schema("public", "audit"));
        assert_eq!(
            child
                .trigger(&identity)
                .unwrap()
                .body()
                .function_reference()
                .unwrap()
                .arguments(),
            &["changed"]
        );
        assert!(
            parent
                .trigger(&identity)
                .unwrap()
                .body()
                .function_reference()
                .unwrap()
                .arguments()
                .is_empty()
        );
        let descendant = test_helpers::layer(&child.freeze());
        assert_eq!(
            descendant
                .get_table_impl(&target)
                .unwrap()
                .triggers()
                .resolve(&descendant, &target)
                .len(),
            2
        );
        assert!(
            test_helpers::apply(
                "CREATE TRIGGER audit BEFORE UPDATE ON items EXECUTE FUNCTION audit_fn()",
                sibling
            )
            .is_ok_and(|pair| matches!(
                pair.ast(),
                crate::sem::SemAst::UnresolvedStatement
            ) && !pair.context().problems().is_empty())
        );
        Ok(())
    }

    #[cfg(feature = "sqlite")]
    #[test]
    fn sqlite_duplicate_namespace_and_if_not_exists() -> testresult::TestResult {
        let root = test_helpers::catalog(Dialect::sqlite()).freeze();
        let context = test_helpers::apply(
            "CREATE TRIGGER Audit AFTER UPDATE ON items BEGIN SELECT NEW.id; END",
            test_helpers::layer(&root),
        )?
        .into_context();
        let key = TriggerIdentity::sqlite(&FullName::with_schema("main", "audit"));
        assert!(context.trigger(&key).is_some());
        let frozen = context.freeze();
        assert!(
            test_helpers::apply(
                "CREATE TRIGGER AUDIT AFTER UPDATE ON other BEGIN SELECT NEW.id; END",
                test_helpers::layer(&frozen)
            )
            .is_ok_and(|pair| matches!(
                pair.ast(),
                crate::sem::SemAst::UnresolvedStatement
            ) && !pair.context().problems().is_empty())
        );
        let context = test_helpers::apply(
            "CREATE TRIGGER IF NOT EXISTS AUDIT AFTER UPDATE ON other BEGIN SELECT NEW.id; END",
            test_helpers::layer(&frozen),
        )?
        .into_context();
        assert_eq!(
            context.trigger(&key).unwrap().target(),
            &FullName::with_schema("main", "items")
        );
        assert!(
            context
                .get_table_impl(&FullName::with_schema("main", "other"))
                .unwrap()
                .triggers()
                .names()
                .is_empty()
        );
        Ok(())
    }

    #[cfg(feature = "sqlite")]
    #[rstest::rstest]
    #[case::postgres(
        Dialect::postgres(),
        "CREATE TRIGGER audit INSTEAD OF UPDATE ON item_view FOR EACH ROW EXECUTE FUNCTION audit_fn()"
    )]
    #[case::sqlite(
        Dialect::sqlite(),
        "CREATE TRIGGER audit INSTEAD OF UPDATE ON item_view BEGIN SELECT NEW.id; END"
    )]
    fn associates_trigger_names_with_views(
        #[case] dialect: Dialect,
        #[case] sql: &str,
    ) -> testresult::TestResult {
        let root = test_helpers::catalog(dialect).freeze();
        let target = if dialect == Dialect::sqlite() {
            FullName::with_schema("main", "item_view")
        } else {
            FullName::with_schema("public", "item_view")
        };
        let context = test_helpers::apply(sql, test_helpers::layer(&root))?.into_context();
        let view = ContextViewInfoRead::get_view_impl(&context, &target).unwrap();
        let references = view.triggers().unwrap();
        assert_eq!(references.names().len(), 1);
        assert_eq!(references.resolve(&context, &target)[0].target(), &target);
        assert!(
            crate::context::ViewInfoRead::get_view_impl(&root, &target)
                .unwrap()
                .triggers()
                .unwrap()
                .names()
                .is_empty()
        );
        Ok(())
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
impl crate::sem::traversal::SemanticVisitorMut for TriggerDependencies {
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
            && let Some(name) = ty.full_name()
            && !self.types.contains(name)
        {
            self.types.push(name.clone());
        }
        Ok(())
    }
}
#[cfg(test)]
mod anonymous_dependency_tests {
    use crate::{
        names::FullName,
        sem::{ScalarConstExpr, SemScalarExpr, create_trigger::TriggerDependencies},
    };

    #[test]
    fn row_dependency_collection_keeps_field_dependencies_without_a_fake_row_name() {
        let field = SemScalarExpr::literal(&ScalarConstExpr::new_integer(1));
        let mut expression = SemScalarExpr::row(&[field]);
        let original = expression.clone();
        let mut dependencies = TriggerDependencies::default();
        expression.walk(&mut dependencies).unwrap();
        assert_eq!(expression, original);
        assert_eq!(
            dependencies.types(),
            &[FullName::with_schema("pg_catalog", "int4")]
        );
        assert!(expression.get_type().unwrap().is_dynamic());
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
                    command.walk_nested_command(dependencies)?;
                }
            }
        }
        Ok(())
    }
}

impl TriggerEvent {
    fn drop_depends_on(&self, context: &impl ParseContext, change: &ColumnDrop) -> bool {
        if let Self::Update(columns) = self {
            columns
                .iter()
                .any(|column| change.matches_name(context, column.name()))
        } else {
            false
        }
    }
}
