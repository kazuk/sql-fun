//! Interpret PostgreSQL function parallel-safety changes.

use crate::{
    ast::{self, ObjectKind, ddl::FunctionOption},
    context::ParseContext,
    sem::{
        AnalysisError, AnalysisProblem, OwnershipLookup, SourcePathWithSpan,
        ownership::RequestedIdentity, traversal::SemanticVisitor,
    },
};

/// PostgreSQL parallel-safety classification of a function.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParallelSafety {
    /// May run in a parallel worker.
    Safe,
    /// May run in the leader of a parallel plan.
    Restricted,
    /// Prevents use of a parallel plan.
    Unsafe,
}

impl TryFrom<&FunctionOption> for ParallelSafety {
    type Error = AnalysisError;

    fn try_from(option: &FunctionOption) -> Result<Self, Self::Error> {
        match option {
            FunctionOption::Parallel(mode) => match mode.as_str() {
                "safe" => Ok(Self::Safe),
                "restricted" => Ok(Self::Restricted),
                "unsafe" => Ok(Self::Unsafe),
                _ => AnalysisError::raise_unexpected_input("invalid function PARALLEL mode"),
            },
            _ => AnalysisError::raise_unexpected_input("expected ALTER FUNCTION PARALLEL action"),
        }
    }
}

/// Analyzed ALTER FUNCTION PARALLEL and its catalog applicability.
#[derive(Clone, Debug)]
pub struct AlterFunctionParallel {
    mode: ParallelSafety,
    target: OwnershipLookup,
    source: Option<SourcePathWithSpan>,
}

impl AlterFunctionParallel {
    pub(crate) fn analyze(
        ast: &ast::AlterFunctionParallel,
        context: &mut impl ParseContext,
    ) -> Result<Self, AnalysisError> {
        let mode = ParallelSafety::try_from(ast.option())?;
        let request =
            RequestedIdentity::from_ast_object_target(ast.target(), ObjectKind::FUNCTION, context)?;
        let target = match request.resolve_routine(context) {
            OwnershipLookup::Resolved(identity) if identity.kind() == ObjectKind::AGGREGATE => {
                OwnershipLookup::invalid_kind(&identity)
            }
            OwnershipLookup::InvalidKind(identity) if ast.target().argument_types().is_none() => {
                OwnershipLookup::not_found(&identity)
            }
            outcome => outcome,
        };
        if target.identity().is_none() {
            context.report_analysis_problem(AnalysisProblem::ownership_target(&target))?;
        }
        Ok(Self {
            mode,
            target,
            source: context.get_source_path_with_span().cloned(),
        })
    }
}

#[cfg(test)]
mod test_target_resolution {
    use crate::{
        ast::StatementNode,
        context::ParseContext,
        names::{FullName, SchemaName},
        sem::{AlterFunctionParallel, OwnershipLookup, PgBuiltInType},
        test_helpers::ownership::{apply_ownership_sql, ownership_context, ownership_statement},
    };

    #[rstest::rstest]
    #[case::omitted_with_shadowing("f", "first", false)]
    #[case::explicit_output_mode("f(IN x integer, OUT y unknown_type)", "first", false)]
    #[case::qualified("second.f(integer)", "second", false)]
    #[case::explicit_zero("f()", "second", true)]
    fn binds_exact_input_identity(
        #[case] target: &str,
        #[case] schema: &str,
        #[case] zero_arguments: bool,
    ) -> testresult::TestResult {
        let mut context = ownership_context();
        context.set_search_path(&[SchemaName::from("first"), SchemaName::from("second")]);
        for sql in [
            "CREATE FUNCTION first.f(IN x integer, OUT y integer) LANGUAGE sql AS 'SELECT x'",
            "CREATE FUNCTION second.f(integer) RETURNS integer LANGUAGE sql AS 'SELECT $1'",
        ] {
            context = apply_ownership_sql(context, sql)?.into_context();
        }
        if zero_arguments {
            context = apply_ownership_sql(
                context,
                "CREATE FUNCTION second.f() RETURNS integer LANGUAGE sql AS 'SELECT 1'",
            )?
            .into_context();
        }
        let sql = format!("ALTER FUNCTION {target} PARALLEL SAFE");
        let StatementNode::AlterFunctionParallel(ast) = ownership_statement(&sql)? else {
            panic!("expected ALTER FUNCTION");
        };
        let statement = AlterFunctionParallel::analyze(&ast, &mut context)?;
        let identity = statement.target().identity().unwrap();
        assert_eq!(identity.name(), &FullName::with_schema(schema, "f"));
        let expected = if zero_arguments {
            vec![]
        } else {
            vec![Some(PgBuiltInType::int4())]
        };
        assert_eq!(identity.arguments(), Some(expected.as_slice()));
        assert!(statement.applicable());
        Ok(())
    }

    #[rstest::rstest]
    #[case::ambiguous("f", "ownership_target_ambiguous")]
    #[case::wrong_kind("first.p(integer)", "ownership_invalid_target_kind")]
    #[case::bare_procedure("p", "ownership_target_not_found")]
    #[case::shadowed_function("p(integer)", "ownership_invalid_target_kind")]
    #[case::missing_signature("f(bigint)", "ownership_target_not_found")]
    #[case::missing_type("f(absent_type)", "ownership_argument_type_not_found")]
    fn retains_rejected_resolution(
        #[case] target: &str,
        #[case] expected: &str,
    ) -> testresult::TestResult {
        let mut context = ownership_context();
        context.set_search_path(&[SchemaName::from("first"), SchemaName::from("second")]);
        for sql in [
            "CREATE FUNCTION first.f(integer) RETURNS integer LANGUAGE sql AS 'SELECT $1'",
            "CREATE FUNCTION second.f(text) RETURNS text LANGUAGE sql AS 'SELECT $1'",
            "CREATE PROCEDURE first.p(integer) LANGUAGE sql AS 'SELECT $1'",
        ] {
            context = apply_ownership_sql(context, sql)?.into_context();
        }
        if target == "p(integer)" {
            context = apply_ownership_sql(
                context,
                "CREATE FUNCTION second.p(integer) RETURNS integer LANGUAGE sql AS 'SELECT $1'",
            )?
            .into_context();
        }
        let sql = format!("ALTER FUNCTION {target} PARALLEL UNSAFE");
        let StatementNode::AlterFunctionParallel(ast) = ownership_statement(&sql)? else {
            panic!("expected ALTER FUNCTION");
        };
        let statement = AlterFunctionParallel::analyze(&ast, &mut context)?;
        assert!(!statement.applicable());
        assert_eq!(statement.target().code(), expected);
        assert_eq!(context.problems().len(), 1);
        assert_eq!(context.problems()[0].code(), expected);
        if let OwnershipLookup::Ambiguous(candidates) = statement.target() {
            assert_eq!(candidates.len(), 2);
        }
        Ok(())
    }
}

impl AlterFunctionParallel {
    /// The requested classification.
    pub const fn mode(&self) -> ParallelSafety {
        self.mode
    }

    /// Completed target binding or the reason the change was rejected.
    pub const fn target(&self) -> &OwnershipLookup {
        &self.target
    }

    /// Whether the catalog change can be applied.
    pub const fn applicable(&self) -> bool {
        self.target.identity().is_some()
    }

    /// Location of the original ALTER statement, when supplied.
    pub const fn source(&self) -> Option<&SourcePathWithSpan> {
        self.source.as_ref()
    }

    pub(crate) fn visit_references(
        &self,
        visitor: &mut impl SemanticVisitor,
    ) -> Result<(), AnalysisError> {
        if let Some(identity) = self.target.identity() {
            visitor.schema_function(identity.name())?;
        }
        Ok(())
    }
}
