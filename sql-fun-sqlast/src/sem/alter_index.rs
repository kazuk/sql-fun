use crate::{
    names::FullName,
    sem::{
        AnalysisError, AnalysisProblem, CreateIndex, Kind, ParseContext,
        problem::{IndexProblem, IndexProblemKind},
        search_path::expand_relation_search_path,
    },
};

/// Completed lookup of an ALTER INDEX target in the shared relation namespace.
#[derive(Clone, Debug)]
pub struct IndexTarget {
    requested: FullName,
    identity: Option<FullName>,
    kind: Option<Kind>,
}
impl IndexTarget {
    fn resolve(context: &impl ParseContext, requested: &FullName) -> Self {
        let found = expand_relation_search_path(context, requested, |name| {
            let kind = context.object_kind(name)?;
            Some((name.clone(), kind))
        });
        let (identity, kind) = match found {
            Some((name, kind)) => (Some(name), Some(kind)),
            None => (None, None),
        };
        Self {
            requested: requested.clone(),
            identity,
            kind,
        }
    }
    /// Spelling requested by the statement, before search-path expansion.
    pub const fn requested(&self) -> &FullName {
        &self.requested
    }
    /// Qualified visible relation identity, including a wrong-kind target.
    pub const fn relation_name(&self) -> Option<&FullName> {
        self.identity.as_ref()
    }
    /// Actual relation kind; None records a completed missing-target lookup.
    pub const fn kind(&self) -> Option<Kind> {
        self.kind
    }
    /// Qualified index binding, present only for a correctly typed target.
    pub fn index_name(&self) -> Option<&FullName> {
        if self.kind == Some(Kind::index()) {
            self.identity.as_ref()
        } else {
            None
        }
    }
    /// Whether no relation was found.
    pub const fn is_missing(&self) -> bool {
        self.kind.is_none()
    }
}

/// One retained storage parameter in a SET or RESET list.
#[derive(Clone, Debug)]
pub struct IndexStorageParameter {
    namespace: Option<String>,
    name: String,
    value: Option<String>,
}
impl IndexStorageParameter {
    pub(crate) fn new(namespace: Option<&str>, name: &str, value: Option<&str>) -> Self {
        Self {
            namespace: namespace.map(str::to_owned),
            name: name.to_owned(),
            value: value.map(str::to_owned),
        }
    }
    /// Explicit namespace, if supplied.
    pub fn namespace(&self) -> Option<&str> {
        self.namespace.as_deref()
    }
    /// Parameter name after SQL identifier folding.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Requested value; None means a bare parameter. RESET rejects supplied values.
    pub fn value(&self) -> Option<&str> {
        self.value.as_deref()
    }
    fn qualified_name(&self) -> String {
        match &self.namespace {
            Some(namespace) => format!("{namespace}.{}", self.name),
            None => self.name.clone(),
        }
    }
    fn problem(&self, target: &FullName, kind: IndexProblemKind) -> IndexProblem {
        IndexProblem::parameter(target, kind, &self.qualified_name(), self.value())
    }
}

/// A retained SET or RESET parameter list.
#[derive(Clone, Debug)]
pub struct IndexStorageChange {
    reset: bool,
    parameters: Vec<IndexStorageParameter>,
}
impl IndexStorageChange {
    pub(crate) fn new(reset: bool, parameters: &[IndexStorageParameter]) -> Self {
        Self {
            reset,
            parameters: parameters.to_vec(),
        }
    }
    /// Whether this is RESET rather than SET.
    pub const fn is_reset(&self) -> bool {
        self.reset
    }
    /// Parameters in statement order, including invalid ones.
    pub fn parameters(&self) -> &[IndexStorageParameter] {
        &self.parameters
    }
    pub(crate) fn evaluate(&self, index: &mut CreateIndex) -> Vec<IndexProblem> {
        if index.access_method() != "btree" {
            todo!("ALTER INDEX storage settings for non-B-tree access methods");
        }
        let mut problems = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for parameter in &self.parameters {
            if self.reset && parameter.value().is_some() {
                problems.push(parameter.problem(index.name(), IndexProblemKind::reset_value()));
                continue;
            }
            let namespace = parameter.namespace();
            if !self.reset && namespace.is_some_and(|namespace| namespace != "toast") {
                problems
                    .push(parameter.problem(index.name(), IndexProblemKind::invalid_namespace()));
                continue;
            }
            // SET ignores toast options on indexes; RESET ignores all namespaces.
            // Retain the parameter in the operation, without a catalog effect.
            if namespace.is_some() {
                continue;
            }
            if !self.reset && !seen.insert(parameter.name()) {
                problems
                    .push(parameter.problem(index.name(), IndexProblemKind::duplicate_parameter()));
                continue;
            }
            if let Err(kind) = index.apply_storage_parameter(parameter, self.reset) {
                problems.push(parameter.problem(index.name(), kind));
            }
        }
        problems
    }
}

/// Expression-column statistics alteration, retaining the requested and effective targets.
#[derive(Clone, Debug)]
pub struct IndexStatisticsChange {
    column: i32,
    requested: i32,
    effective: Option<i32>,
}
impl IndexStatisticsChange {
    pub(crate) const fn new(column: i32, requested: i32) -> Self {
        Self {
            column,
            requested,
            effective: None,
        }
    }
    /// Requested one-based column ordinal.
    pub const fn column(&self) -> i32 {
        self.column
    }
    /// Original statistics target, including -1 for reset.
    pub const fn requested_target(&self) -> i32 {
        self.requested
    }
    /// Validated target, including clamping and -1 for reset; None if invalid or skipped.
    pub const fn effective_target(&self) -> Option<i32> {
        self.effective
    }
    fn evaluate(&mut self, index: &mut CreateIndex) -> Vec<IndexProblem> {
        let invalid = if self.column <= 0 || self.column > 32767 {
            Some(IndexProblemKind::invalid_column())
        } else {
            index.statistics_column_problem(self.column)
        };
        if let Some(kind) = invalid {
            return vec![IndexProblem::statistics(
                index.name(),
                kind,
                self.column,
                self.requested,
            )];
        }
        if self.requested < -1 {
            return vec![IndexProblem::statistics(
                index.name(),
                IndexProblemKind::statistics_too_low(),
                self.column,
                self.requested,
            )];
        }
        let effective = self.requested.min(10000);
        self.effective = Some(effective);
        index.apply_statistics(self.column, effective);
        if effective != self.requested {
            vec![IndexProblem::statistics(
                index.name(),
                IndexProblemKind::statistics_clamped(),
                self.column,
                self.requested,
            )]
        } else {
            Vec::new()
        }
    }
}

/// An analyzed index command with its structured inputs.
#[derive(Clone, Debug)]
pub enum IndexAlteration {
    /// SET or RESET storage parameters.
    Storage(IndexStorageChange),
    /// Statistics on a numbered expression column.
    Statistics(IndexStatisticsChange),
}
impl IndexAlteration {
    pub(crate) fn storage(reset: bool, parameters: &[IndexStorageParameter]) -> Self {
        Self::Storage(IndexStorageChange::new(reset, parameters))
    }
    pub(crate) const fn statistics(column: i32, target: i32) -> Self {
        Self::Statistics(IndexStatisticsChange::new(column, target))
    }
    /// Inspect a SET or RESET operation.
    pub const fn storage_change(&self) -> Option<&IndexStorageChange> {
        if let Self::Storage(change) = self {
            Some(change)
        } else {
            None
        }
    }
    /// Inspect a statistics operation.
    pub const fn statistics_change(&self) -> Option<&IndexStatisticsChange> {
        if let Self::Statistics(change) = self {
            Some(change)
        } else {
            None
        }
    }
    fn evaluate(&mut self, index: &mut CreateIndex) -> Vec<IndexProblem> {
        match self {
            Self::Storage(change) => change.evaluate(index),
            Self::Statistics(change) => change.evaluate(index),
        }
    }
}

/// A completed ALTER INDEX analysis and its atomic catalog replacement, if applicable.
#[derive(Clone, Debug)]
pub struct AlterIndex {
    target: IndexTarget,
    if_exists: bool,
    operations: Vec<IndexAlteration>,
    problems: Vec<IndexProblem>,
    replacement: Option<CreateIndex>,
}
impl AlterIndex {
    pub(crate) fn resolve(
        context: &mut impl ParseContext,
        requested: &FullName,
        if_exists: bool,
        operations: &[IndexAlteration],
    ) -> Result<Self, AnalysisError> {
        let target = IndexTarget::resolve(context, requested);
        let mut replacement = if let Some(name) = target.index_name() {
            let Some(index) = context.get_index_impl(name) else {
                return AnalysisError::raise_unexpected_none("resolved index definition");
            };
            Some(index.clone())
        } else {
            None
        };
        let mut operations = operations.to_vec();
        let mut problems = Vec::new();
        if let Some(index) = &mut replacement {
            for operation in &mut operations {
                problems.extend(operation.evaluate(index));
            }
        } else if !target.is_missing() {
            let Some(name) = target.relation_name() else {
                return AnalysisError::raise_unexpected_none("visible index target");
            };
            problems.push(IndexProblem::target(name, IndexProblemKind::wrong_kind()));
        } else if !if_exists {
            problems.push(IndexProblem::target(
                requested,
                IndexProblemKind::missing_target(),
            ));
        }
        for problem in &problems {
            if !problem.is_warning() {
                replacement = None;
            }
            context.report_analysis_problem(AnalysisProblem::index(problem))?;
        }
        Ok(Self {
            target,
            if_exists,
            operations,
            problems,
            replacement,
        })
    }
    /// Completed typed lookup outcome.
    pub const fn target(&self) -> &IndexTarget {
        &self.target
    }
    /// Whether IF EXISTS was explicitly supplied.
    pub const fn if_exists(&self) -> bool {
        self.if_exists
    }
    /// Retained operations in application order.
    pub fn operations(&self) -> &[IndexAlteration] {
        &self.operations
    }
    /// Validation outcomes, including non-fatal warnings.
    pub fn problems(&self) -> &[IndexProblem] {
        &self.problems
    }
    /// Whether validation permits this statement's catalog replacement.
    pub const fn is_applicable(&self) -> bool {
        self.replacement.is_some()
    }
    /// Whether an absent target was explicitly allowed by IF EXISTS.
    pub const fn is_skipped(&self) -> bool {
        self.if_exists && self.target.is_missing()
    }
    /// Validated replacement. Application must publish this definition without repeating analysis.
    pub const fn replacement(&self) -> Option<&CreateIndex> {
        self.replacement.as_ref()
    }
}

#[cfg(test)]
mod tests;
