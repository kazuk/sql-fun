use crate::sem::scalar_expr::SemScalarExprNode;
use crate::sem::{
    AnalysisError, AnalysisProblem, InputTypeConstraint, ParameterConstraints, ParseContext,
    SelectStatement, TypeReference, problem::ArrayProblem,
};

/// ARRAY(query) with its analyzed child and array-result domain.
/// Successful construction is non-null even for zero rows or NULL elements.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ArraySubqueryExpr {
    query: Box<SelectStatement>,
    types: InputTypeConstraint,
    invalid_shape: bool,
    #[serde(skip)]
    node: Option<usize>,
}

impl ArraySubqueryExpr {
    pub(crate) fn resolve(
        context: &mut impl ParseContext,
        query: &SelectStatement,
    ) -> Result<Self, AnalysisError> {
        let mut value = Self {
            query: Box::new(query.clone()),
            types: InputTypeConstraint::any(),
            invalid_shape: false,
            node: None,
        };
        value.query.walk(&mut UnknownOutput { context, depth: 0 })?;
        value.validate_shape(context)?;
        value.refresh_type();
        Ok(value)
    }

    /// The analyzed query, including completed name bindings and query clauses.
    #[must_use]
    pub fn query(&self) -> &SelectStatement {
        &self.query
    }

    /// Array-result candidates; unrestricted while unresolved, empty on failure.
    #[must_use]
    pub const fn types(&self) -> &InputTypeConstraint {
        &self.types
    }

    fn validate_shape(&mut self, context: &mut impl ParseContext) -> Result<(), AnalysisError> {
        if !self.invalid_shape && !self.query.has_pending_stars() && !self.query.has_width(1) {
            self.invalid_shape = true;
            self.types = InputTypeConstraint::types([]);
            context.report_analysis_problem(AnalysisProblem::array(
                ArrayProblem::subquery_columns(self.query.result_columns().len()),
            ))?;
        }
        Ok(())
    }

    fn refresh_type(&mut self) {
        if self.node.is_none() && !self.invalid_shape && !self.query.has_pending_stars() {
            self.types = self
                .query
                .result_columns()
                .scalar_type()
                .map(|ty| InputTypeConstraint::exact(&ty.array_type()))
                .unwrap_or_else(InputTypeConstraint::any);
        }
    }

    pub(crate) fn expand_pending_stars(
        &mut self,
        context: &mut impl ParseContext,
    ) -> Result<bool, AnalysisError> {
        let changed = self.query.expand_pending_stars(context)?;
        self.validate_shape(context)?;
        Ok(changed)
    }

    pub(crate) fn walk<V: crate::sem::traversal::SemanticVisitorMut>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        self.query.walk(visitor)?;
        visitor.array_subquery(self)?;
        self.refresh_type();
        Ok(())
    }
    pub(crate) fn visit<V: crate::sem::traversal::SemanticVisitor>(
        &self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        self.query.visit(visitor)?;
        visitor.array_subquery(self)?;
        Ok(())
    }

    pub(crate) fn collect_constraints(
        &mut self,
        parameters: &mut crate::sem::parameter_resolution::ParameterResolution<'_>,
        context: &mut impl ParseContext,
    ) -> Result<(), AnalysisError> {
        self.validate_shape(context)?;
        let single_output = !self.invalid_shape && !self.query.has_pending_stars();
        let fallback = if self.invalid_shape {
            InputTypeConstraint::types([])
        } else if single_output {
            self.query
                .result_columns()
                .scalar_type()
                .map(|ty| InputTypeConstraint::exact(&ty.array_type()))
                .unwrap_or_else(InputTypeConstraint::any)
        } else {
            InputTypeConstraint::any()
        };
        let (node, types) = parameters.array_query_result(&fallback, single_output, self.node);
        self.node = Some(node);
        self.types = types;
        Ok(())
    }

    pub(crate) const fn node(&self) -> Option<usize> {
        self.node
    }

    pub(crate) fn apply_constraints(&mut self, constraints: &ParameterConstraints) {
        if let Some(node) = self.node {
            self.types = constraints.node_domain(node).clone();
        }
    }

    pub(super) const fn canonicalize_grouping_key(&mut self) {
        self.node = None;
    }
}

impl SemScalarExprNode for ArraySubqueryExpr {
    fn get_type(&self) -> Option<TypeReference> {
        self.types.single_type()
    }

    fn is_not_null(&self) -> Option<bool> {
        if self.types.admissible_types() == Some(&[]) || self.query.has_pending_stars() {
            None
        } else {
            Some(true)
        }
    }
}

// PostgreSQL resolves an untyped NULL projection to text at the query boundary.
// Limit this normalization to this ARRAY query; nested query analysis keeps its
// own output/coercion rules and unresolved parameters retain their domains.
struct UnknownOutput<'a, C: ParseContext> {
    context: &'a C,
    depth: usize,
}

impl<C: ParseContext> crate::sem::traversal::SemanticVisitorMut for UnknownOutput<'_, C> {
    fn enter_query(&mut self) {
        self.depth += 1;
    }
    fn leave_query(&mut self) {
        self.depth -= 1;
    }
    fn output(&mut self, _name: &str, value: &mut crate::sem::SemScalarExpr) {
        if self.context.dialect() == crate::ast::Dialect::postgres()
            && self.depth == 1
            && value
                .as_const()
                .is_some_and(crate::sem::ScalarConstExpr::is_null)
        {
            *value = crate::sem::SemScalarExpr::wrap_implicit_cast(
                &crate::sem::PgBuiltInType::text(),
                value,
            );
        }
    }
}
