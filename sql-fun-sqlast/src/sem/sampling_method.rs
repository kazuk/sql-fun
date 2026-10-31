use crate::sem::{FullName, TypeReference};

/// Offline facts for one PostgreSQL `(internal) RETURNS tsm_handler` handler.
/// The parameter types describe TABLESAMPLE inputs, not the dummy handler argument.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SamplingMethod {
    name: FullName,
    parameter_types: Vec<TypeReference>,
    repeatable: bool,
}
impl SamplingMethod {
    /// Record verified handler identity, ordered input types and cross-query repeatability.
    pub fn new(name: &FullName, parameter_types: &[TypeReference], repeatable: bool) -> Self {
        Self {
            name: name.clone(),
            parameter_types: parameter_types.to_vec(),
            repeatable,
        }
    }
    /// Canonical, schema-qualified handler identity.
    pub const fn name(&self) -> &FullName {
        &self.name
    }
    pub(crate) fn input_targets(&self, count: usize) -> Option<&[TypeReference]> {
        if self.parameter_types.len() == count {
            Some(&self.parameter_types)
        } else {
            None
        }
    }
    /// Ordered types accepted by TABLESAMPLE.
    pub fn parameter_types(&self) -> &[TypeReference] {
        &self.parameter_types
    }
    /// Whether the handler accepts an explicit REPEATABLE clause.
    pub const fn supports_repeatable(&self) -> bool {
        self.repeatable
    }
}

/// Completed method lookup, including a catalog's inability to supply its facts.
/// Unavailable metadata is not evidence that a method does not exist.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum SamplingMethodLookup {
    /// A verified handler and its sampling facts.
    Found(SamplingMethod),
    /// The selected catalog establishes that the method is absent.
    NotFound,
    /// The selected catalog cannot establish the requested method's facts.
    Unavailable,
    /// More than one eligible handler remains after visibility rules.
    Ambiguous(Vec<FullName>),
    /// The selected handler does not have the required SQL handler contract.
    InvalidHandler(FullName),
}
impl SamplingMethodLookup {
    /// Retain a successfully resolved handler.
    pub fn found(method: &SamplingMethod) -> Self {
        Self::Found(method.clone())
    }
    /// Record conclusive absence.
    pub const fn not_found() -> Self {
        Self::NotFound
    }
    /// Record unavailable catalog facts without fabricating a binding.
    pub const fn unavailable() -> Self {
        Self::Unavailable
    }
    /// Retain ambiguous handler candidates.
    pub fn ambiguous(candidates: &[FullName]) -> Self {
        Self::Ambiguous(candidates.to_vec())
    }
    /// Retain a selected handler with an invalid signature or return type.
    pub fn invalid_handler(name: &FullName) -> Self {
        Self::InvalidHandler(name.clone())
    }
    /// Resolved, verified sampling facts, if available.
    pub const fn method(&self) -> Option<&SamplingMethod> {
        if let Self::Found(method) = self {
            Some(method)
        } else {
            None
        }
    }
    /// Whether the catalog could not establish the requested facts.
    pub const fn is_unavailable(&self) -> bool {
        matches!(self, Self::Unavailable)
    }
    /// Whether the catalog conclusively established absence.
    pub const fn is_not_found(&self) -> bool {
        matches!(self, Self::NotFound)
    }
    pub(crate) fn validate_clause(
        &self,
        context: &mut impl crate::sem::ParseContext,
        name: &FullName,
        count: usize,
        repeatable: bool,
    ) -> Result<(), crate::sem::AnalysisError> {
        use crate::sem::{AnalysisProblem, problem::TableSampleProblem};
        if let Some(problem) = self.problem(name) {
            context.report_analysis_problem(AnalysisProblem::table_sample(problem))?;
        }
        if let Some(method) = self.method() {
            if method.parameter_types.len() != count {
                context.report_analysis_problem(AnalysisProblem::table_sample(
                    TableSampleProblem::arity(&format!(
                        "{name}: expected {}, supplied {count}",
                        method.parameter_types.len()
                    )),
                ))?;
            }
            if repeatable && !method.supports_repeatable() {
                context.report_analysis_problem(AnalysisProblem::table_sample(
                    TableSampleProblem::repeatable(&name.to_string()),
                ))?;
            }
        }
        Ok(())
    }
    pub(crate) fn problem(
        &self,
        name: &FullName,
    ) -> Option<crate::sem::problem::TableSampleProblem> {
        use crate::sem::problem::TableSampleProblem;
        match self {
            Self::Found(_) => None,
            Self::NotFound => Some(TableSampleProblem::method_not_found(&name.to_string())),
            Self::Unavailable => Some(TableSampleProblem::metadata_unavailable(&name.to_string())),
            Self::Ambiguous(_) => Some(TableSampleProblem::method_ambiguous(&name.to_string())),
            Self::InvalidHandler(_) => Some(TableSampleProblem::invalid_handler(&name.to_string())),
        }
    }
}
