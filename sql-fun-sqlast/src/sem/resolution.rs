//! Catalog reads and diagnostic collection for expression/query resolution.

use crate::sem::{
    AnalysisError, AnalysisProblem, BaseParseContext, CastDefinition, CreateTable,
    DiagnosticContext, FullName, FunctionOverloadCollection, OperatorDefinition, OperatorName,
    ParseContext, TableName, TypeDefinition, TypeReference, ViewLike, ViewName,
};

use std::collections::HashSet;

/// The resolver cannot obtain a mutable catalog through this interface.
pub(crate) trait Catalog {
    fn cast_sources(&self, target: &TypeReference, explicit: bool) -> HashSet<TypeReference>;
    fn metadata(&self) -> &::sql_fun_core::SqlFunMetadata;
    fn has_index(&self, name: &FullName) -> bool;
    fn has_sequence(&self, name: &FullName) -> bool;
    fn resolve_function_name(&self, name: &FullName) -> FullName;
    fn get_type(&self, name: &FullName) -> Option<&TypeDefinition>;
    fn get_table(&self, name: &TableName) -> Option<&CreateTable>;
    fn resolve_table_name(&self, name: &TableName) -> Option<TableName>;
    fn get_view(&self, name: &ViewName) -> Option<&ViewLike>;
    fn get_function_by_name(&self, name: &FullName) -> Option<FunctionOverloadCollection>;
    fn get_operator_op(
        &self,
        name: &OperatorName,
        left: bool,
        right: bool,
    ) -> Option<&OperatorDefinition>;
    fn get_array_item_type(&self, name: &TypeReference) -> Option<TypeReference>;
    fn get_explicit_cast(
        &self,
        source: &TypeReference,
        target: &TypeReference,
    ) -> Option<CastDefinition>;
    fn get_implicit_cast(
        &self,
        source: &TypeReference,
        target: &TypeReference,
    ) -> Option<CastDefinition>;
    fn get_implicit_castable(&self, source: &TypeReference) -> HashSet<TypeReference>;
    fn canonicalize_type_ref(&self, value: &TypeReference) -> Option<TypeReference>;
}

impl<T: BaseParseContext> Catalog for T {
    fn cast_sources(&self, target: &TypeReference, explicit: bool) -> HashSet<TypeReference> {
        crate::sem::CastInfoRead::cast_sources(self, target, explicit)
    }
    fn metadata(&self) -> &::sql_fun_core::SqlFunMetadata {
        BaseParseContext::metadata(self)
    }
    fn has_index(&self, name: &FullName) -> bool {
        BaseParseContext::get_index(self, name).is_some()
    }
    fn has_sequence(&self, name: &FullName) -> bool {
        BaseParseContext::get_sequence(self, name).is_some()
    }
    fn resolve_function_name(&self, name: &FullName) -> FullName {
        BaseParseContext::resolve_function_name_with_search_path(self, name)
    }
    fn get_type(&self, name: &FullName) -> Option<&TypeDefinition> {
        BaseParseContext::get_type(self, name)
    }
    fn resolve_table_name(&self, name: &TableName) -> Option<TableName> {
        BaseParseContext::resolve_table_name(self, name)
    }
    fn get_table(&self, name: &TableName) -> Option<&CreateTable> {
        BaseParseContext::get_table(self, name)
    }
    fn get_view(&self, name: &ViewName) -> Option<&ViewLike> {
        BaseParseContext::get_view(self, name)
    }
    fn get_function_by_name(&self, name: &FullName) -> Option<FunctionOverloadCollection> {
        BaseParseContext::get_function_by_name(self, name)
    }
    fn get_operator_op(
        &self,
        name: &OperatorName,
        left: bool,
        right: bool,
    ) -> Option<&OperatorDefinition> {
        crate::sem::OperatorInfoRead::get_operator_op(self, name, left, right)
    }
    fn get_array_item_type(&self, name: &TypeReference) -> Option<TypeReference> {
        crate::sem::TypeInfoRead::get_array_item_type(self, name)
    }
    fn get_explicit_cast(
        &self,
        source: &TypeReference,
        target: &TypeReference,
    ) -> Option<CastDefinition> {
        crate::sem::CastInfoRead::get_explicit_cast(self, source, target)
    }
    fn get_implicit_cast(
        &self,
        source: &TypeReference,
        target: &TypeReference,
    ) -> Option<CastDefinition> {
        crate::sem::CastInfoRead::get_implicit_cast(self, source, target)
    }
    fn get_implicit_castable(&self, source: &TypeReference) -> HashSet<TypeReference> {
        crate::sem::CastInfoRead::get_implicit_castable(self, source)
    }
    fn canonicalize_type_ref(&self, value: &TypeReference) -> Option<TypeReference> {
        BaseParseContext::canonicalize_type_ref(self, value)
    }
}

/// Mutation is limited to an analysis-local diagnostic sink.
pub(crate) trait ResolutionContext: Catalog {
    fn report_problem(&mut self, problem: AnalysisProblem) -> Result<(), AnalysisError>;
}

impl<T: ParseContext> ResolutionContext for T {
    fn report_problem(&mut self, problem: AnalysisProblem) -> Result<(), AnalysisError> {
        DiagnosticContext::report_problem(self, problem)
    }
}

pub(crate) struct Resolution<'a> {
    catalog: &'a dyn Catalog,
    problems: Vec<AnalysisProblem>,
    parameters: crate::ast::ParameterResolution,
}

impl<'a> Resolution<'a> {
    pub(crate) fn parameter_function_signatures(
        &self,
        overloads: &FunctionOverloadCollection,
        arguments: &[crate::sem::SemScalarExpr],
    ) -> Vec<crate::ast::Signature> {
        overloads.parameter_signatures(self, arguments)
    }

    pub(crate) const fn problem_count(&self) -> usize {
        self.problems.len()
    }

    pub(crate) const fn parameters(&mut self) -> &mut crate::ast::ParameterResolution {
        &mut self.parameters
    }
    pub(crate) fn solve_parameters(&mut self) -> Result<bool, AnalysisError> {
        let replay = self.parameters.solve()?;
        if replay {
            self.problems.clear();
        }
        Ok(replay)
    }
    pub(crate) fn parameter_require(
        &mut self,
        value: &crate::sem::SemScalarExpr,
        target: &TypeReference,
        explicit: bool,
    ) {
        self.parameters
            .require(self.catalog, value, target, explicit);
    }
    pub(crate) fn parameter_call(
        &mut self,
        name: &str,
        arguments: &[crate::sem::SemScalarExpr],
        signatures: &[crate::ast::Signature],
    ) -> Result<crate::sem::SemScalarExpr, AnalysisError> {
        self.parameters
            .call(self.catalog, name, arguments, signatures)
    }

    pub(crate) fn new(catalog: &'a dyn Catalog) -> Self {
        Self {
            catalog,
            problems: Vec::new(),
            parameters: crate::ast::ParameterResolution::default(),
        }
    }

    pub(crate) fn finish<T>(self, result: Result<T, AnalysisError>) -> ResolutionOutcome<T> {
        ResolutionOutcome {
            result,
            parameters: self.parameters.constraints(),
            problems: self.problems,
        }
    }
}

pub(crate) struct ResolutionOutcome<T> {
    result: Result<T, AnalysisError>,
    parameters: crate::sem::ParameterConstraints,
    problems: Vec<AnalysisProblem>,
}

impl<T> ResolutionOutcome<T> {
    pub(crate) fn map<U>(self, f: impl FnOnce(T) -> U) -> ResolutionOutcome<U> {
        ResolutionOutcome {
            result: self.result.map(f),
            parameters: self.parameters,
            problems: self.problems,
        }
    }

    pub(crate) fn report<C: DiagnosticContext>(self, context: &mut C) -> Result<T, AnalysisError> {
        self.report_with_parameters(context).map(|(value, _)| value)
    }
    pub(crate) fn report_with_parameters<C: DiagnosticContext>(
        self,
        context: &mut C,
    ) -> Result<(T, crate::sem::ParameterConstraints), AnalysisError> {
        for problem in self.problems {
            context.report_problem(problem)?;
        }
        self.result.map(|value| (value, self.parameters))
    }
}

#[cfg(test)]
mod test_resolution_outcome {
    use crate::sem::resolution::{Resolution, ResolutionContext};
    use crate::sem::{AnalysisError, AnalysisProblem, FullName};
    use crate::test_helpers::TestParseContext;

    #[test]
    fn retains_ordered_diagnostics_before_fatal_result() -> testresult::TestResult {
        let catalog = TestParseContext::default();
        let mut resolution = Resolution::new(&catalog);
        let first = AnalysisProblem::function_not_found(
            &FullName::from_local_name("first"),
            &crate::StringSpan::new(3, 5),
        );
        let second = AnalysisProblem::function_not_found(
            &FullName::from_local_name("second"),
            &crate::StringSpan::new(12, 6),
        );
        resolution.report_problem(first.clone())?;
        resolution.report_problem(second.clone())?;
        let fatal: Result<(), AnalysisError> =
            Err(std::io::Error::other("resolution failed").into());
        let mut context = TestParseContext::default();
        let error = resolution
            .finish(fatal)
            .report(&mut context)
            .expect_err("fatal");
        assert_eq!(error.to_string(), "resolution failed");
        assert_eq!(
            format!("{:?}", context.reported_problems()),
            format!("{:?}", [first, second])
        );
        Ok(())
    }

    #[test]
    fn reporting_failure_takes_precedence_and_stops_replay() -> testresult::TestResult {
        let catalog = TestParseContext::default();
        let mut resolution = Resolution::new(&catalog);
        for name in ["first", "second", "third"] {
            resolution.report_problem(AnalysisProblem::function_not_found(
                &FullName::from_local_name(name),
                &crate::StringSpan::new(0, 0),
            ))?;
        }
        let mut context = TestParseContext::default();
        context.reject_diagnostics_after(1);
        let fatal: Result<(), AnalysisError> = Err(std::io::Error::other("later fatal").into());
        let error = resolution
            .finish(fatal)
            .report(&mut context)
            .expect_err("sink failure");
        assert_eq!(error.to_string(), "diagnostic sink rejected a problem");
        assert_eq!(context.reported_problem_count(), 1);
        Ok(())
    }
}

impl Catalog for Resolution<'_> {
    fn cast_sources(&self, target: &TypeReference, explicit: bool) -> HashSet<TypeReference> {
        self.catalog.cast_sources(target, explicit)
    }
    fn metadata(&self) -> &::sql_fun_core::SqlFunMetadata {
        self.catalog.metadata()
    }
    fn has_index(&self, name: &FullName) -> bool {
        self.catalog.has_index(name)
    }
    fn has_sequence(&self, name: &FullName) -> bool {
        self.catalog.has_sequence(name)
    }
    fn resolve_function_name(&self, name: &FullName) -> FullName {
        self.catalog.resolve_function_name(name)
    }
    fn get_type(&self, name: &FullName) -> Option<&TypeDefinition> {
        self.catalog.get_type(name)
    }
    fn resolve_table_name(&self, name: &TableName) -> Option<TableName> {
        self.catalog.resolve_table_name(name)
    }
    fn get_table(&self, name: &TableName) -> Option<&CreateTable> {
        self.catalog.get_table(name)
    }
    fn get_view(&self, name: &ViewName) -> Option<&ViewLike> {
        self.catalog.get_view(name)
    }
    fn get_function_by_name(&self, name: &FullName) -> Option<FunctionOverloadCollection> {
        self.catalog.get_function_by_name(name)
    }
    fn get_operator_op(
        &self,
        name: &OperatorName,
        left: bool,
        right: bool,
    ) -> Option<&OperatorDefinition> {
        self.catalog.get_operator_op(name, left, right)
    }
    fn get_array_item_type(&self, name: &TypeReference) -> Option<TypeReference> {
        self.catalog.get_array_item_type(name)
    }
    fn get_explicit_cast(
        &self,
        source: &TypeReference,
        target: &TypeReference,
    ) -> Option<CastDefinition> {
        self.catalog.get_explicit_cast(source, target)
    }
    fn get_implicit_cast(
        &self,
        source: &TypeReference,
        target: &TypeReference,
    ) -> Option<CastDefinition> {
        self.catalog.get_implicit_cast(source, target)
    }
    fn get_implicit_castable(&self, source: &TypeReference) -> HashSet<TypeReference> {
        self.catalog.get_implicit_castable(source)
    }
    fn canonicalize_type_ref(&self, value: &TypeReference) -> Option<TypeReference> {
        self.catalog.canonicalize_type_ref(value)
    }
}

impl ResolutionContext for Resolution<'_> {
    fn report_problem(&mut self, problem: AnalysisProblem) -> Result<(), AnalysisError> {
        self.problems.push(problem);
        Ok(())
    }
}

impl Catalog for &mut Resolution<'_> {
    fn cast_sources(&self, target: &TypeReference, explicit: bool) -> HashSet<TypeReference> {
        self.catalog.cast_sources(target, explicit)
    }
    fn metadata(&self) -> &::sql_fun_core::SqlFunMetadata {
        self.catalog.metadata()
    }
    fn has_index(&self, name: &FullName) -> bool {
        Catalog::has_index(&**self, name)
    }
    fn has_sequence(&self, name: &FullName) -> bool {
        Catalog::has_sequence(&**self, name)
    }
    fn resolve_function_name(&self, name: &FullName) -> FullName {
        self.catalog.resolve_function_name(name)
    }
    fn get_type(&self, name: &FullName) -> Option<&TypeDefinition> {
        self.catalog.get_type(name)
    }
    fn resolve_table_name(&self, name: &TableName) -> Option<TableName> {
        Catalog::resolve_table_name(&**self, name)
    }
    fn get_table(&self, name: &TableName) -> Option<&CreateTable> {
        self.catalog.get_table(name)
    }
    fn get_view(&self, name: &ViewName) -> Option<&ViewLike> {
        self.catalog.get_view(name)
    }
    fn get_function_by_name(&self, name: &FullName) -> Option<FunctionOverloadCollection> {
        self.catalog.get_function_by_name(name)
    }
    fn get_operator_op(
        &self,
        name: &OperatorName,
        left: bool,
        right: bool,
    ) -> Option<&OperatorDefinition> {
        self.catalog.get_operator_op(name, left, right)
    }
    fn get_array_item_type(&self, name: &TypeReference) -> Option<TypeReference> {
        self.catalog.get_array_item_type(name)
    }
    fn get_explicit_cast(
        &self,
        source: &TypeReference,
        target: &TypeReference,
    ) -> Option<CastDefinition> {
        self.catalog.get_explicit_cast(source, target)
    }
    fn get_implicit_cast(
        &self,
        source: &TypeReference,
        target: &TypeReference,
    ) -> Option<CastDefinition> {
        self.catalog.get_implicit_cast(source, target)
    }
    fn get_implicit_castable(&self, source: &TypeReference) -> HashSet<TypeReference> {
        self.catalog.get_implicit_castable(source)
    }
    fn canonicalize_type_ref(&self, value: &TypeReference) -> Option<TypeReference> {
        self.catalog.canonicalize_type_ref(value)
    }
}

impl ResolutionContext for &mut Resolution<'_> {
    fn report_problem(&mut self, problem: AnalysisProblem) -> Result<(), AnalysisError> {
        self.problems.push(problem);
        Ok(())
    }
}
