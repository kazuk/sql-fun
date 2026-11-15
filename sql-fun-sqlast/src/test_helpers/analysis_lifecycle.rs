use crate::{
    AstAndContextPair, StringSpan,
    ast::Dialect,
    context::{ParseContext, SchemaFileContext},
    names::{FullName, OperatorName},
    sem::{
        AnalysisError, AnalysisProblem, BinaryOperatorDefinition, MembershipExpr,
        OperatorDefinition, PgBuiltInType, SelectStatement, SemAst, SemScalarExpr,
        traversal::SemanticVisitor,
    },
    test_helpers::{
        create_cache_context,
        public_analysis::statement,
        scalar_catalog::{categorized_scalar_catalog, schema_from_base},
    },
};
pub(crate) fn missing_functions() -> Vec<AnalysisProblem> {
    vec![
        AnalysisProblem::function_not_found(
            &FullName::from_local_name("first"),
            &StringSpan::new(3, 5),
        ),
        AnalysisProblem::function_not_found(
            &FullName::from_local_name("second"),
            &StringSpan::new(12, 6),
        ),
    ]
}
pub(crate) fn report_missing_functions(
    context: &mut impl ParseContext,
) -> Result<(), AnalysisError> {
    for problem in missing_functions() {
        context.report_analysis_problem(problem)?;
    }
    Ok(())
}

#[derive(Default)]
struct InferenceNodes(Vec<usize>);

impl SemanticVisitor for InferenceNodes {
    fn set_output(&mut self, _index: usize, _name: &str, expression: &SemScalarExpr) {
        expression
            .visit(self)
            .expect("inspect completed set output");
    }

    fn expression(&mut self, expression: &SemScalarExpr) -> Result<(), AnalysisError> {
        if let Some(node) = expression.constraint_node() {
            self.0.push(node);
        }
        Ok(())
    }

    fn membership(&mut self, value: &MembershipExpr) -> Result<(), AnalysisError> {
        for comparison in value.comparisons() {
            comparison.left().visit(self)?;
            comparison.right().visit(self)?;
            if let Some(expression) = comparison.expression() {
                expression.visit(self)?;
            }
        }
        Ok(())
    }
}

pub(crate) fn inference_nodes(query: &SelectStatement) -> Vec<usize> {
    let mut nodes = InferenceNodes::default();
    query.visit(&mut nodes).expect("inspect inference lifetime");
    nodes.0
}

/// Small scalar catalog with equality for inference lifetime tests.
pub(crate) fn analyze_inference(sql: &str) -> AstAndContextPair<SchemaFileContext> {
    let mut context = categorized_scalar_catalog(Dialect::postgres());
    let integer = PgBuiltInType::int4();
    context.setup_type(integer.array_type());
    context.setup_ordered_type(integer.full_name().expect("nominal fixture type"));
    context.set_get_operator_op_result(
        &OperatorName::from_parts(None, "="),
        true,
        true,
        Some(&OperatorDefinition::binary(&[
            BinaryOperatorDefinition::new(&integer, &integer, &PgBuiltInType::bool(), true),
        ])),
    );
    SemAst::from_ast(
        &statement(sql, Dialect::postgres()),
        schema_from_base(context, std::path::Path::new("inference.sql")),
        &mut create_cache_context(),
    )
    .expect("analyze inference fixture")
}
