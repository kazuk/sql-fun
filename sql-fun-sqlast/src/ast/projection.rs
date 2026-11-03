//! Owning access to projection collections and their lexical source scopes.

use crate::{
    ast::{
        Alias, DerivedTable, FromGroup, FromItem, Join, Relation, StatementNode, TableFunction,
        TableFunctions, TableSample, Target,
        traversal::{AstVisitorMut, WalkSqlMut},
    },
    stack::with_stack,
};

#[derive(Debug)]
pub(in crate::ast) enum ProjectionError {
    Missing,
    Ambiguous,
    Duplicate,
    WrongKind,
    EmptyReturning,
    Incompatible,
}
impl ProjectionError {
    pub(in crate::ast) const fn is_missing(&self) -> bool {
        matches!(self, Self::Missing)
    }
    pub(in crate::ast) const fn is_ambiguous(&self) -> bool {
        matches!(self, Self::Ambiguous)
    }
    pub(in crate::ast) const fn is_duplicate(&self) -> bool {
        matches!(self, Self::Duplicate)
    }
    pub(in crate::ast) const fn is_wrong_kind(&self) -> bool {
        matches!(self, Self::WrongKind)
    }
    pub(in crate::ast) const fn is_empty_returning(&self) -> bool {
        matches!(self, Self::EmptyReturning)
    }
    const fn ambiguous() -> Self {
        Self::Ambiguous
    }
    const fn duplicate() -> Self {
        Self::Duplicate
    }
    const fn wrong_kind() -> Self {
        Self::WrongKind
    }
    const fn empty_returning() -> Self {
        Self::EmptyReturning
    }
    const fn incompatible() -> Self {
        Self::Incompatible
    }
    const fn missing<T>() -> Result<T, Self> {
        Err(Self::Missing)
    }
}

pub(in crate::ast) struct ProjectionScope<'a> {
    targets: &'a mut Vec<Target>,
    sources: &'a mut [FromItem],
    returning: bool,
}
impl<'a> ProjectionScope<'a> {
    pub(in crate::ast) const fn new(
        targets: &'a mut Vec<Target>,
        sources: &'a mut [FromItem],
        returning: bool,
    ) -> Self {
        Self {
            targets,
            sources,
            returning,
        }
    }
    fn locations(&self) -> LocationGroup {
        LocationGroup::new(self.returning, self.targets)
    }
    fn source_snapshot(&self) -> SourceSet {
        SourceSet::new(self.sources)
    }
    fn replace_source(&mut self, alias: &str, transform: &mut impl FnMut(&FromItem) -> FromItem) {
        for source in &mut *self.sources {
            source.visit_visible_alias(alias, &mut |source| {
                *source = transform(source);
            });
            source.normalize_projection_groups();
        }
    }
}
struct LocationGroup {
    returning: bool,
    locations: Vec<Option<usize>>,
}
impl LocationGroup {
    fn new(returning: bool, targets: &[Target]) -> Self {
        Self {
            returning,
            locations: targets.iter().map(Target::projection_location).collect(),
        }
    }
    const fn corresponds(&self, scope: &ProjectionScope<'_>) -> bool {
        scope.has_layout(self.returning, self.locations.len())
    }
    fn contains(&self, location: usize) -> bool {
        self.locations.contains(&Some(location))
    }
    fn apply(&self, scope: &mut ProjectionScope<'_>) {
        scope.synchronize_locations(&self.locations);
    }
    fn validate_locations(
        &self,
        unique: &mut std::collections::HashSet<usize>,
    ) -> Result<(), ProjectionError> {
        for location in &self.locations {
            let Some(location) = location else {
                return ProjectionError::missing();
            };
            if !unique.insert(*location) {
                return Err(ProjectionError::ambiguous());
            }
        }
        Ok(())
    }
}
impl Target {
    pub(in crate::ast) const fn projection_location(&self) -> Option<usize> {
        self.location
    }
    const fn set_projection_location(&mut self, location: Option<usize>) {
        self.location = location;
    }
}

pub(in crate::ast) struct ProjectionLocations {
    groups: Vec<LocationGroup>,
}
impl ProjectionLocations {
    pub(in crate::ast) fn capture(statement: &mut StatementNode) -> Self {
        let mut capture = Self { groups: Vec::new() };
        statement.walk_sql_mut(&mut capture);
        capture
    }
    pub(in crate::ast) fn validate_source(&self) -> Result<(), ProjectionError> {
        let mut unique = std::collections::HashSet::new();
        for group in &self.groups {
            group.validate_locations(&mut unique)?;
        }
        Ok(())
    }
    pub(in crate::ast) fn validate_destination(
        &self,
        statement: &mut StatementNode,
    ) -> Result<(), ProjectionError> {
        let mut visitor = LocationCorrespondence::new(self);
        statement.walk_sql_mut(&mut visitor);
        if visitor.complete(self.groups.len()) {
            Ok(())
        } else {
            Err(ProjectionError::incompatible())
        }
    }
    pub(in crate::ast) fn apply(&self, statement: &mut StatementNode) {
        let mut visitor = LocationApplication::new(self);
        statement.walk_sql_mut(&mut visitor);
    }
}
impl AstVisitorMut for ProjectionLocations {
    fn enter_projection(&mut self, scope: &mut ProjectionScope<'_>) {
        self.groups.push(scope.locations());
    }
}
struct LocationCorrespondence<'a> {
    source: &'a ProjectionLocations,
    index: usize,
    compatible: bool,
}
impl AstVisitorMut for LocationCorrespondence<'_> {
    fn enter_projection(&mut self, scope: &mut ProjectionScope<'_>) {
        self.compatible &= self.source.corresponds_at(self.index, scope);
        self.index += 1;
    }
}
struct LocationApplication<'a> {
    source: &'a ProjectionLocations,
    index: usize,
}
impl AstVisitorMut for LocationApplication<'_> {
    fn enter_projection(&mut self, scope: &mut ProjectionScope<'_>) {
        self.source.apply_at(self.index, scope);
        self.index += 1;
    }
}

struct ProjectionReplacement {
    location: usize,
    targets: Vec<Target>,
}
impl ProjectionReplacement {
    fn new(location: usize, targets: &[Target]) -> Self {
        Self {
            location,
            targets: targets.to_vec(),
        }
    }
}
pub(in crate::ast) struct ProjectionReplacements {
    edits: Vec<ProjectionReplacement>,
}
impl ProjectionReplacements {
    pub(in crate::ast) const fn new() -> Self {
        Self { edits: Vec::new() }
    }
    pub(in crate::ast) fn add(
        &mut self,
        location: usize,
        targets: &[Target],
    ) -> Result<(), ProjectionError> {
        if self.edits.iter().any(|edit| edit.location == location) {
            return Err(ProjectionError::duplicate());
        }
        self.edits
            .push(ProjectionReplacement::new(location, targets));
        Ok(())
    }
    pub(in crate::ast) const fn len(&self) -> usize {
        self.edits.len()
    }
    pub(in crate::ast) fn validate_wildcards(
        &self,
        statement: &mut StatementNode,
    ) -> Result<(), ProjectionError> {
        let mut visitor = ReplacementValidation::new(self);
        statement.walk_sql_mut(&mut visitor);
        visitor.validate()
    }
    pub(in crate::ast) fn apply(&mut self, statement: &mut StatementNode) {
        statement.walk_sql_mut(self);
    }
}
struct ProjectionCheck {
    location: usize,
    width: usize,
    count: usize,
    wrong_kind: bool,
}
impl ProjectionCheck {
    const fn new(location: usize, width: usize) -> Self {
        Self {
            location,
            width,
            count: 0,
            wrong_kind: false,
        }
    }
    fn observe(&mut self, target: &Target) -> Option<usize> {
        if target.projection_location() != Some(self.location) {
            return None;
        }
        self.count += 1;
        self.wrong_kind |= !target.is_projection_star();
        Some(self.width)
    }
    const fn validate(&self) -> Result<(), ProjectionError> {
        if self.count == 0 {
            return ProjectionError::missing();
        }
        if self.count != 1 {
            return Err(ProjectionError::ambiguous());
        }
        if self.wrong_kind {
            return Err(ProjectionError::wrong_kind());
        }
        Ok(())
    }
}
struct ReplacementValidation {
    checks: Vec<ProjectionCheck>,
    empty_returning: bool,
}
impl ReplacementValidation {
    fn new(replacements: &ProjectionReplacements) -> Self {
        Self {
            checks: replacements.checks(),
            empty_returning: false,
        }
    }
    fn observe(&mut self, target: &Target) -> Option<usize> {
        for check in &mut self.checks {
            if let Some(width) = check.observe(target) {
                return Some(width);
            }
        }
        None
    }
    const fn check_result(&mut self, returning: bool, original_width: usize, width: usize) {
        self.empty_returning |= returning && original_width > 0 && width == 0;
    }
    fn validate(&self) -> Result<(), ProjectionError> {
        for check in &self.checks {
            check.validate()?;
        }
        if self.empty_returning {
            return Err(ProjectionError::empty_returning());
        }
        Ok(())
    }
}
impl AstVisitorMut for ReplacementValidation {
    fn enter_projection(&mut self, scope: &mut ProjectionScope<'_>) {
        scope.check_replacements(self);
    }
}
impl AstVisitorMut for ProjectionReplacements {
    fn enter_projection(&mut self, scope: &mut ProjectionScope<'_>) {
        scope.apply_replacements(self);
    }
}
impl ProjectionScope<'_> {
    const fn has_layout(&self, returning: bool, width: usize) -> bool {
        self.returning == returning && self.targets.len() == width
    }
    fn synchronize_locations(&mut self, locations: &[Option<usize>]) {
        for (target, location) in self.targets.iter_mut().zip(locations) {
            target.set_projection_location(*location);
        }
    }
    fn check_replacements(&self, validation: &mut ReplacementValidation) {
        let mut width = self.targets.len();
        for target in &*self.targets {
            if let Some(replacement_width) = validation.observe(target) {
                width = width - 1 + replacement_width;
            }
        }
        validation.check_result(self.returning, self.targets.len(), width);
    }
    fn apply_replacements(&mut self, replacements: &mut ProjectionReplacements) {
        for index in (0..self.targets.len()).rev() {
            if let Some(targets) = replacements.take_for(self.targets[index].projection_location())
            {
                self.targets.splice(index..=index, targets);
            }
        }
    }
}
impl ProjectionReplacement {
    const fn check(&self) -> ProjectionCheck {
        ProjectionCheck::new(self.location, self.targets.len())
    }
}
impl ProjectionReplacements {
    fn checks(&self) -> Vec<ProjectionCheck> {
        self.edits
            .iter()
            .map(ProjectionReplacement::check)
            .collect()
    }
    fn take_for(&mut self, location: Option<usize>) -> Option<Vec<Target>> {
        let edit = self
            .edits
            .iter_mut()
            .find(|edit| Some(edit.location) == location)?;
        Some(std::mem::take(&mut edit.targets))
    }
}

#[derive(Clone)]
struct SourceSet(Vec<FromItem>);
impl SourceSet {
    fn source(&self, alias: &str) -> Result<FromItem, ProjectionError> {
        let mut found = Vec::new();
        for source in &self.0 {
            let mut source = source.clone();
            source.visit_visible_alias(alias, &mut |source| found.push(source.clone()));
        }
        if found.len() > 1 {
            return Err(ProjectionError::ambiguous());
        }
        match found.pop() {
            Some(source) => Ok(source),
            None => ProjectionError::missing(),
        }
    }
}
struct ScopeRecord {
    parent: Option<usize>,
    locations: LocationGroup,
    sources: SourceSet,
}
impl ScopeRecord {
    fn new(parent: Option<usize>, scope: &ProjectionScope<'_>) -> Self {
        Self {
            parent,
            locations: scope.locations(),
            sources: scope.source_snapshot(),
        }
    }
}
pub(in crate::ast) struct ProjectionScopes {
    records: Vec<ScopeRecord>,
    stack: Vec<usize>,
}
impl ProjectionScopes {
    pub(in crate::ast) fn capture(statement: &mut StatementNode) -> Self {
        let mut scopes = Self {
            records: Vec::new(),
            stack: Vec::new(),
        };
        statement.walk_sql_mut(&mut scopes);
        scopes
    }
    pub(in crate::ast) fn source_for_target(
        &self,
        location: usize,
        depth: usize,
        alias: &str,
    ) -> Result<ScopedSource, ProjectionError> {
        let mut scopes = self
            .records
            .iter()
            .enumerate()
            .filter(|(_, scope)| scope.contains_location(location));
        let Some((mut index, _)) = scopes.next() else {
            return ProjectionError::missing();
        };
        if scopes.next().is_some() {
            return Err(ProjectionError::ambiguous());
        }
        for _ in 0..depth {
            let Some(parent) = self.records[index].parent else {
                return ProjectionError::missing();
            };
            index = parent;
        }
        self.records[index].validate_alias(alias)?;
        Ok(ScopedSource::new(index))
    }
}
impl AstVisitorMut for ProjectionScopes {
    fn enter_projection(&mut self, scope: &mut ProjectionScope<'_>) {
        let index = self.records.len();
        self.records
            .push(ScopeRecord::new(self.stack.last().copied(), scope));
        self.stack.push(index);
    }
    fn leave_projection(&mut self, _scope: &mut ProjectionScope<'_>) {
        self.stack.pop();
    }
}
pub(in crate::ast) struct ScopedSource {
    scope: usize,
}
impl ScopedSource {
    const fn new(scope: usize) -> Self {
        Self { scope }
    }
    pub(in crate::ast) const fn scope(&self) -> usize {
        self.scope
    }
}
pub(in crate::ast) trait ScopedSourceEdit {
    fn replace_sources(&mut self, scope: usize, projection: &mut ProjectionScope<'_>);
}
struct SourceApplication<'a, T> {
    edits: &'a mut T,
    next: usize,
    stack: Vec<usize>,
}
impl<T: ScopedSourceEdit> AstVisitorMut for SourceApplication<'_, T> {
    fn enter_projection(&mut self, _scope: &mut ProjectionScope<'_>) {
        self.stack.push(self.next);
        self.next += 1;
    }
    fn leave_projection(&mut self, scope: &mut ProjectionScope<'_>) {
        self.edits
            .replace_sources(self.stack.pop().expect("entered scope"), scope);
    }
}
impl StatementNode {
    pub(in crate::ast) fn apply_source_edits(&mut self, edits: &mut impl ScopedSourceEdit) {
        self.walk_sql_mut(&mut SourceApplication::new(edits));
    }
}

impl FromItem {
    pub(in crate::ast) fn visit_visible_alias(
        &mut self,
        alias: &str,
        visitor: &mut impl FnMut(&mut Self),
    ) {
        with_stack(|| {
            if let Some(name) = self.visible_alias() {
                if name == alias {
                    visitor(self);
                }
                return;
            }
            self.visit_unaliased_children(alias, visitor);
        })
    }
    fn visible_alias(&self) -> Option<&str> {
        match self {
            Self::Relation(node) => node.visible_alias(),
            Self::Query(node) => node.visible_alias(),
            Self::Join(node) => node.visible_alias(),
            Self::Functions(node) => node.visible_alias(),
            Self::Group(node) => node.visible_alias(),
            Self::Sample(node) => node.visible_alias(),
            Self::Xml(node) => node.visible_alias(),
            Self::Json(node) => node.visible_alias(),
        }
    }
    fn visit_unaliased_children(&mut self, alias: &str, visitor: &mut impl FnMut(&mut Self)) {
        match self {
            Self::Join(node) => node.visit_visible_children(alias, visitor),
            Self::Group(node) => node.visit_visible_children(alias, visitor),
            _ => {}
        }
    }
    pub(in crate::ast) fn normalize_projection_groups(&mut self) {
        with_stack(|| {
            let normalized = match self {
                Self::Join(node) => node.normalize_source(),
                Self::Group(node) => node.normalize_source(),
                _ => None,
            };
            if let Some(source) = normalized {
                *self = source;
            }
        })
    }
    const fn is_derived_source(&self) -> bool {
        matches!(self, Self::Query(_))
    }
}
impl Alias {
    pub(in crate::ast) fn visible_name(&self) -> &str {
        &self.name
    }
}
impl Relation {
    fn visible_alias(&self) -> Option<&str> {
        if let Some(alias) = &self.alias {
            Some(alias.visible_name())
        } else {
            self.name.projection_local_name()
        }
    }
}
impl DerivedTable {
    fn visible_alias(&self) -> Option<&str> {
        self.alias.as_ref().map(Alias::visible_name)
    }
}
impl Join {
    fn visible_alias(&self) -> Option<&str> {
        self.alias.as_ref().map(Alias::visible_name)
    }
    fn visit_visible_children(&mut self, alias: &str, visitor: &mut impl FnMut(&mut FromItem)) {
        self.left.visit_visible_alias(alias, visitor);
        self.right.visit_visible_alias(alias, visitor);
    }
    fn normalize_projection_groups(&mut self) {
        self.left.normalize_projection_groups();
        self.right.normalize_projection_groups();
    }
}
impl FromGroup {
    fn visible_alias(&self) -> Option<&str> {
        self.alias.as_ref().map(Alias::visible_name)
    }
    fn visit_visible_children(&mut self, alias: &str, visitor: &mut impl FnMut(&mut FromItem)) {
        for source in &mut self.items {
            source.visit_visible_alias(alias, visitor);
        }
    }
    fn normalize_projection_groups(&mut self) {
        for source in &mut self.items {
            source.normalize_projection_groups();
        }
    }
    fn take_single_derived_source(&mut self) -> Option<FromItem> {
        if self.alias.is_none() && self.items.len() == 1 && self.items[0].is_derived_source() {
            self.items.pop()
        } else {
            None
        }
    }
}
impl TableFunctions {
    fn visible_alias(&self) -> Option<&str> {
        if let Some(alias) = &self.alias {
            Some(alias.visible_name())
        } else if let Some(call) = self.calls.first() {
            call.visible_alias()
        } else {
            None
        }
    }
}
impl TableFunction {
    fn visible_alias(&self) -> Option<&str> {
        self.expression.projection_function_alias()
    }
}
impl TableSample {
    fn visible_alias(&self) -> Option<&str> {
        self.relation.visible_alias()
    }
}
impl FromItem {
    pub(in crate::ast) fn visible_source(&self, alias: &str) -> Result<Self, ProjectionError> {
        SourceSet::single(self).source(alias)
    }
    pub(in crate::ast) fn replace_visible_source(&mut self, alias: &str, replacement: &Self) {
        self.visit_visible_alias(alias, &mut |source| *source = replacement.clone());
        self.normalize_projection_groups();
    }
}
impl SourceSet {
    fn single(source: &FromItem) -> Self {
        Self(vec![source.clone()])
    }
}

impl Target {
    const fn is_projection_star(&self) -> bool {
        self.expression.is_projection_star()
    }
}
impl ProjectionLocations {
    fn corresponds_at(&self, index: usize, scope: &ProjectionScope<'_>) -> bool {
        self.groups
            .get(index)
            .is_some_and(|group| group.corresponds(scope))
    }
    fn apply_at(&self, index: usize, scope: &mut ProjectionScope<'_>) {
        self.groups[index].apply(scope);
    }
}
impl<'a> LocationCorrespondence<'a> {
    const fn new(source: &'a ProjectionLocations) -> Self {
        Self {
            source,
            index: 0,
            compatible: true,
        }
    }
    const fn complete(&self, group_count: usize) -> bool {
        self.compatible && self.index == group_count
    }
}
impl<'a> LocationApplication<'a> {
    const fn new(source: &'a ProjectionLocations) -> Self {
        Self { source, index: 0 }
    }
}
impl SourceSet {
    fn new(sources: &[FromItem]) -> Self {
        Self(sources.to_vec())
    }
}

impl<'a, T> SourceApplication<'a, T> {
    const fn new(edits: &'a mut T) -> Self {
        Self {
            edits,
            next: 0,
            stack: Vec::new(),
        }
    }
}

impl ScopeRecord {
    fn validate_alias(&self, alias: &str) -> Result<(), ProjectionError> {
        self.sources.source(alias)?;
        Ok(())
    }
    fn contains_location(&self, location: usize) -> bool {
        self.locations.contains(location)
    }
}

impl Join {
    fn normalize_source(&mut self) -> Option<FromItem> {
        self.normalize_projection_groups();
        None
    }
}
impl FromGroup {
    fn normalize_source(&mut self) -> Option<FromItem> {
        self.normalize_projection_groups();
        self.take_single_derived_source()
    }
}
impl ProjectionScope<'_> {
    pub(in crate::ast) fn try_transform_source<E: From<ProjectionError>>(
        &mut self,
        alias: &str,
        transform: &mut impl FnMut(&FromItem) -> Result<FromItem, E>,
    ) -> Result<(), E> {
        let source = self.source_snapshot().source(alias).map_err(E::from)?;
        let replacement = transform(&source)?;
        self.replace_source(alias, &mut |_| replacement.clone());
        Ok(())
    }
}
