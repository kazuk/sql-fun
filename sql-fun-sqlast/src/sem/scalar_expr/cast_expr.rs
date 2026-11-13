use crate::{
    ast::{Dialect, Expr},
    context::BaseParseContext,
    names::TableName,
    sem::{
        AnalysisError, InputTypeConstraint, ParseContext, QueryNameResolution, SemScalarExpr,
        TypeReference, WildcardQualifiers, WildcardRewriteError,
        parameter_resolution::ParameterResolution,
        scalar_expr::SemScalarExprNode,
        traversal::{SemanticVisitor, SemanticVisitorMut},
    },
};

/// implicit cast expression
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ImplicitCastExpr {
    value: Box<SemScalarExpr>,
    cast_target: TypeReference,
}

impl ImplicitCastExpr {
    pub(crate) fn rename_relation(&mut self, old: &TableName, new: &TableName) {
        self.cast_target.rename_relation(old, new);
    }

    pub(crate) fn projection_operand(&self) -> (&SemScalarExpr, &TypeReference) {
        (&self.value, &self.cast_target)
    }

    /// create a implicit cast expression
    #[must_use]
    pub fn new(value: &SemScalarExpr, cast_target: &TypeReference) -> Self {
        Self {
            value: Box::new(value.clone()),
            cast_target: cast_target.clone(),
        }
    }
}

impl SemScalarExprNode for ImplicitCastExpr {
    fn get_type(&self) -> Option<TypeReference> {
        Some(self.cast_target.clone())
    }

    fn is_not_null(&self) -> Option<bool> {
        self.value.is_not_null()
    }
}

/// explicit cast expression
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TypeCastExpr {
    target_type: TypeReference,
    resolution: QueryNameResolution,
    arg: Box<SemScalarExpr>,
}

impl TypeCastExpr {
    // Keep the possible top-level call depth and whether identity is still pending.
    pub(super) fn table_root_depth(&self) -> (usize, bool) {
        let (depth, pending) = self.arg.table_root_depth();
        if depth == 0 {
            return (0, false);
        }
        let Some(source) = self.arg.placement_result_type() else {
            return (depth + 1, true);
        };
        if source == self.target_type {
            (depth + 1, pending)
        } else {
            (0, false)
        }
    }
}

impl TypeCastExpr {
    pub(crate) fn rename_relation(&mut self, old: &TableName, new: &TableName) {
        self.target_type.rename_relation(old, new);
        self.resolution.rename_relation(old, new);
    }

    pub(crate) fn projection_operand(&self) -> (&SemScalarExpr, &TypeReference) {
        (&self.arg, &self.target_type)
    }

    pub(super) fn record_operand(&self) -> Option<&SemScalarExpr> {
        if self.target_type == TypeReference::from_master_type_name("record") {
            Some(&self.arg)
        } else {
            None
        }
    }

    pub(super) fn constructor_projection_name(&self) -> Option<&'static str> {
        self.arg.constructor_projection_name()
    }

    pub(super) fn grouping_key_operand(&self) -> Option<&SemScalarExpr> {
        if self.resolution.is_resolved() && self.arg.get_type().as_ref() == Some(&self.target_type)
        {
            Some(&self.arg)
        } else {
            None
        }
    }

    pub(crate) fn set_resolution(&mut self, resolution: QueryNameResolution) {
        self.resolution = resolution;
    }
    /// Completed target-name resolution (or SQLite affinity interpretation).
    pub const fn resolution(&self) -> &QueryNameResolution {
        &self.resolution
    }

    pub(super) fn from_resolved(target_type: &TypeReference, arg: &SemScalarExpr) -> Self {
        Self {
            target_type: target_type.clone(),
            resolution: QueryNameResolution::type_reference(target_type),
            arg: Box::new(arg.clone()),
        }
    }
}

impl SemScalarExprNode for TypeCastExpr {
    fn get_type(&self) -> Option<TypeReference> {
        Some(self.target_type.clone())
    }

    fn is_not_null(&self) -> Option<bool> {
        self.arg.is_not_null()
    }
}

impl TypeCastExpr {
    pub(crate) fn walk<V: SemanticVisitorMut>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        self.arg.walk(visitor)?;
        Ok(())
    }
    pub(crate) fn visit<V: SemanticVisitor>(&self, visitor: &mut V) -> Result<(), AnalysisError> {
        self.resolution.visit_type(visitor)?;
        self.arg.visit(visitor)?;
        Ok(())
    }
}

impl ImplicitCastExpr {
    pub(crate) fn walk<V: SemanticVisitorMut>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        self.value.walk(visitor)?;
        Ok(())
    }
    pub(crate) fn visit<V: SemanticVisitor>(&self, visitor: &mut V) -> Result<(), AnalysisError> {
        visitor.schema_type(&self.cast_target)?;
        self.value.visit(visitor)?;
        Ok(())
    }
}

impl TypeCastExpr {
    pub(crate) fn collect_constraints(
        &mut self,
        parameters: &mut ParameterResolution<'_>,
        context: &mut impl ParseContext,
    ) -> Result<Option<SemScalarExpr>, AnalysisError> {
        if self.arg.constraint_node().is_none() {
            return Ok(None);
        }
        let target = if context.dialect() == Dialect::postgres() {
            if let Some(target) =
                BaseParseContext::canonicalize_type_ref(context, &self.target_type)
            {
                parameters.require(context, &self.arg, &target, true);
                target
            } else {
                self.target_type.clone()
            }
        } else {
            self.target_type.clone()
        };
        Ok(Some(parameters.operation(
            "CAST",
            std::slice::from_ref(&self.arg),
            &InputTypeConstraint::exact(&target),
            None,
        )))
    }
}

#[cfg(test)]
mod test_sqlite_casts_do_not_require_postgres_casts {
    #[cfg(feature = "sqlite")]
    use crate::{
        ast::Dialect,
        context::ParseContext,
        names::FullName,
        parse_statements,
        sem::{SemAst, TypeReference},
        test_helpers::{TestParseContext, create_cache_context},
    };

    #[cfg(feature = "sqlite")]
    #[rstest::rstest]
    #[case::projection("select cast(?1 as integer) as n")]
    #[case::cte("with x as (select cast(?1 as integer) as n) select n from x")]
    fn sqlite_casts_do_not_require_postgres_casts(#[case] sql: &str) {
        let mut cache_context = create_cache_context();
        let mut context = TestParseContext::default().with_dialect(Dialect::sqlite());
        context.setup_type(TypeReference::from_full_name(FullName::from_local_name(
            "integer",
        )));
        let result = SemAst::from_ast(
            &parse_statements(sql, Dialect::sqlite())
                .next()
                .unwrap()
                .unwrap(),
            context,
            &mut cache_context,
        )
        .unwrap();
        assert!(
            result
                .context()
                .parameter_constraints()
                .parameter(1)
                .unwrap()
                .constraint()
                .is_any()
        );
        assert_eq!(result.context().reported_problem_count(), 0);
        let select = result.ast().clone().into_select_statemtnt().unwrap();
        assert!(
            select
                .result_columns()
                .get_column_at(0)
                .unwrap()
                .col_type()
                .is_some()
        );
    }
}

impl ImplicitCastExpr {
    pub(crate) fn projection_expression(
        &self,
        bindings: &WildcardQualifiers,
    ) -> Result<Expr, WildcardRewriteError> {
        let (argument, target) = self.projection_operand();
        argument.projection_cast(target, bindings)
    }
}

impl TypeCastExpr {
    pub(crate) fn projection_expression(
        &self,
        bindings: &WildcardQualifiers,
    ) -> Result<Expr, WildcardRewriteError> {
        let (argument, target) = self.projection_operand();
        argument.projection_cast(target, bindings)
    }
}
