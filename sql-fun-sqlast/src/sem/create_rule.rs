use crate::sem::traversal::SemanticVisitor;
use crate::sem::{AnalysisError, BaseParseContext, FullName, ParseContext, SemAst, SemScalarExpr};

/// PostgreSQL rule identity: a rule name in one resolved relation's namespace.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RuleIdentity {
    target: FullName,
    name: FullName,
}
impl RuleIdentity {
    /// Construct from a resolved target and the logical local rule name.
    pub fn new(target: &FullName, name: &str) -> Self {
        Self {
            target: target.clone(),
            name: FullName::new(
                &target.catalog().cloned(),
                &target.schema().cloned(),
                &crate::sem::LocalName::from(name),
            ),
        }
    }
    /// Owning relation identity.
    pub const fn target(&self) -> &FullName {
        &self.target
    }
    /// Qualified definition name; the target also participates in identity.
    pub const fn name(&self) -> &FullName {
        &self.name
    }
}

/// Target-owned links to rule definitions in the effective context.
#[derive(Debug, Clone, Default)]
pub struct RuleReferences(Vec<RuleIdentity>);
impl RuleReferences {
    /// Exact linked definition identities, including any masked historical link.
    pub fn identities(&self) -> &[RuleIdentity] {
        &self.0
    }
    /// Currently effective analyzed definitions, in attachment order.
    /// Tombstones are excluded, and replacement uses the current definition.
    pub fn resolve<'a>(&self, context: &'a impl BaseParseContext) -> Vec<&'a CreateRule> {
        self.0
            .iter()
            .filter_map(|identity| context.rule(identity))
            .collect()
    }
    pub(crate) fn associate(&mut self, identity: &RuleIdentity) {
        if !self.0.contains(identity) {
            self.0.push(identity.clone());
        }
    }
    pub(crate) fn dissociate(&mut self, identity: &RuleIdentity) {
        self.0.retain(|existing| existing != identity);
    }
}

/// Operation to which a PostgreSQL rule applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleEvent {
    /// View read definition.
    Select,
    /// Insertion.
    Insert,
    /// Update.
    Update,
    /// Deletion.
    Delete,
}
impl RuleEvent {
    /// SELECT event.
    pub const fn select() -> Self {
        Self::Select
    }
    /// INSERT event.
    pub const fn insert() -> Self {
        Self::Insert
    }
    /// UPDATE event.
    pub const fn update() -> Self {
        Self::Update
    }
    /// DELETE event.
    pub const fn delete() -> Self {
        Self::Delete
    }

    pub(crate) const fn write_event(self) -> Option<crate::sem::view_write::WriteEvent> {
        match self {
            Self::Select => None,
            Self::Insert => Some(crate::sem::view_write::WriteEvent::insert()),
            Self::Update => Some(crate::sem::view_write::WriteEvent::update()),
            Self::Delete => Some(crate::sem::view_write::WriteEvent::delete()),
        }
    }
    pub(crate) fn permits_row(self, alias: &str) -> bool {
        matches!(
            (self, alias),
            (Self::Insert | Self::Update, "new") | (Self::Update | Self::Delete, "old")
        )
    }
}

/// Completed CREATE RULE semantics, independent of applying or executing it.
#[derive(Debug, Clone)]
pub struct CreateRule {
    identity: RuleIdentity,
    event: RuleEvent,
    replace: bool,
    instead: bool,
    predicate: Option<SemScalarExpr>,
    actions: Vec<SemAst>,
    dependencies: RuleDependencies,
    source: Option<crate::sem::SourcePathWithSpan>,
    select_lineage: Option<crate::sem::ViewColumnLineage>,
}
impl CreateRule {
    pub(crate) fn new(
        identity: &RuleIdentity,
        event: RuleEvent,
        replace: bool,
        instead: bool,
        predicate: &Option<SemScalarExpr>,
        actions: &[SemAst],
        context: &impl ParseContext,
    ) -> Result<Self, AnalysisError> {
        let mut definition = Self {
            identity: identity.clone(),
            event,
            replace,
            instead,
            predicate: predicate.clone(),
            actions: actions.to_vec(),
            dependencies: RuleDependencies::default(),
            source: context.get_source_path_with_span().cloned(),
            select_lineage: None,
        };
        definition.retain_dependencies()?;
        Ok(definition)
    }
    pub(crate) fn replace_view_definition(
        context: &impl ParseContext,
        target: &FullName,
        query: &crate::sem::SelectStatement,
    ) -> Result<Option<Self>, AnalysisError> {
        let identity = RuleIdentity::new(target, "_RETURN");
        if context.rule(&identity).is_none() {
            return Ok(None);
        }
        Self::new(
            &identity,
            RuleEvent::select(),
            true,
            true,
            &None,
            &[SemAst::select(query)],
            context,
        )
        .map(Some)
    }
    pub(crate) fn retain_select_lineage(&mut self, lineage: &crate::sem::ViewColumnLineage) {
        self.select_lineage = Some(lineage.clone());
    }
    /// Registered object identity.
    pub const fn identity(&self) -> &RuleIdentity {
        &self.identity
    }
    /// Resolved target relation.
    pub const fn target(&self) -> &FullName {
        self.identity.target()
    }
    /// Rule event.
    pub const fn event(&self) -> RuleEvent {
        self.event
    }
    /// Whether OR REPLACE was requested.
    pub const fn replaces(&self) -> bool {
        self.replace
    }
    /// Whether actions replace the triggering operation (otherwise ALSO).
    pub const fn instead(&self) -> bool {
        self.instead
    }
    /// Analyzed condition, absent for an unconditional rule.
    pub const fn predicate(&self) -> Option<&SemScalarExpr> {
        self.predicate.as_ref()
    }
    /// Analyzed actions in source order. An empty slice means DO NOTHING.
    pub fn actions(&self) -> &[SemAst] {
        &self.actions
    }
    /// Bound dependencies without repeating name lookup.
    pub const fn dependencies(&self) -> &RuleDependencies {
        &self.dependencies
    }
    /// Source location, when provided by the caller.
    pub const fn source(&self) -> Option<&crate::sem::SourcePathWithSpan> {
        self.source.as_ref()
    }
    pub(crate) const fn replaces_unconditionally(&self) -> bool {
        self.instead && self.predicate.is_none()
    }
    pub(crate) fn has_returning(&self) -> bool {
        self.actions
            .iter()
            .filter_map(SemAst::rule_returning)
            .any(|columns| !columns.is_empty())
    }
    fn retain_dependencies(&mut self) -> Result<(), AnalysisError> {
        let mut dependencies = RuleDependencies::default();
        self.walk(&mut dependencies)?;
        self.dependencies = dependencies;
        Ok(())
    }
    pub(crate) fn walk<V: SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        visitor.relation_identity(self.identity.target());
        if let Some(predicate) = &mut self.predicate {
            predicate.walk(visitor)?;
        }
        for action in &mut self.actions {
            action.walk_nested_command(visitor)?;
        }
        Ok(())
    }
    pub(crate) fn apply<C: ParseContext>(&self, mut context: C) -> Result<C, AnalysisError> {
        if !self.replace && context.rule(&self.identity).is_some() {
            context.report_analysis_problem(crate::sem::AnalysisProblem::rule(
                &crate::sem::problem::RuleProblem::duplicate(&self.identity),
            ))?;
            return Ok(context);
        }
        if context.resolve_relation_name(self.target()).is_none() {
            context.report_analysis_problem(crate::sem::AnalysisProblem::rule(
                &crate::sem::problem::RuleProblem::target(self.target()),
            ))?;
            return Ok(context);
        }
        // DDL chooses both operations; storage does not interpret rule semantics.
        let mut context = context.associate_rule(self.identity.target(), &self.identity)?;
        let context = if self.event == RuleEvent::select() {
            let Some(view) = context.get_view(&crate::sem::ViewName::from_full_name(self.target()))
            else {
                context.report_analysis_problem(crate::sem::AnalysisProblem::rule(
                    &crate::sem::problem::RuleProblem::target(self.target()),
                ))?;
                return Ok(context);
            };
            let query = self.actions[0]
                .select_statement()
                .expect("validated SELECT rule action");
            let view = view
                .with_rule_query(query)
                .expect("validated ordinary view");
            let view = if let Some(lineage) = &self.select_lineage {
                view.with_column_lineage(lineage)
            } else {
                view
            };
            let context = context.apply_create_view(&view)?;
            context.associate_rule(self.target(), &self.identity)?
        } else {
            context
        };
        context.register_rule(self)
    }
}

/// Catalog dependencies retained by an analyzed rule.
#[derive(Debug, Clone, Default)]
pub struct RuleDependencies {
    relations: Vec<FullName>,
    functions: Vec<FullName>,
    types: Vec<FullName>,
}
impl RuleDependencies {
    /// Resolved relation identities.
    pub fn relations(&self) -> &[FullName] {
        &self.relations
    }
    /// Bound function identities.
    pub fn functions(&self) -> &[FullName] {
        &self.functions
    }
    /// Bound expression types.
    pub fn types(&self) -> &[FullName] {
        &self.types
    }
}
impl SemanticVisitor for RuleDependencies {
    fn relation_identity(&mut self, name: &FullName) {
        if !self.relations.contains(name) {
            self.relations.push(name.clone());
        }
    }
    fn expression(&mut self, expression: &mut SemScalarExpr) -> Result<(), AnalysisError> {
        if let Some(name) = expression.function_dependency()
            && !self.functions.contains(name)
        {
            self.functions.push(name.clone());
        }
        if let Some(ty) = expression.get_type()
            && !ty.is_dynamic()
            && !self.types.contains(ty.full_name())
        {
            self.types.push(ty.full_name().clone());
        }
        Ok(())
    }
}
