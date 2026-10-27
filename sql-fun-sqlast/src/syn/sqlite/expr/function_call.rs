use crate::syn::sqlite::expr::Expr;
use crate::syn::sqlite::{Distinctness, FunctionTail, Name, SortedColumn};
use crate::{engine_specific::TursoOrderedSetAggregateFeature, sem::AnalysisError};

/// function call expression
#[derive(Clone, Debug, derive_getters::Getters)]
pub struct ExprFunctionCall {
    name: Name,
    distinctness: Option<Distinctness>,
    args: Vec<Expr>,
    order_by: Vec<SortedColumn>,
    #[getter(skip)]
    within_group: Vec<SortedColumn>,
    filter_over: FunctionTail,
}

impl ExprFunctionCall {
    pub(crate) fn syntax_within_group(&self) -> &[SortedColumn] {
        &self.within_group
    }

    /// create instance
    pub fn new(
        name: &Name,
        distinctness: &Option<Distinctness>,
        args: &[Expr],
        order_by: &[SortedColumn],
        within_group: &[SortedColumn],
        filter_over: &FunctionTail,
    ) -> Self {
        Self {
            name: name.clone(),
            distinctness: distinctness.clone(),
            args: args.to_vec(),
            order_by: order_by.to_vec(),
            within_group: within_group.to_vec(),
            filter_over: filter_over.clone(),
        }
    }

    /// Ordered-set ordering, notifying the engine-specific hook when present.
    pub fn within_group<Gate: TursoOrderedSetAggregateFeature>(
        &self,
        gate: &mut Gate,
    ) -> Result<&[SortedColumn], AnalysisError> {
        if !self.within_group.is_empty() {
            gate.ordered_set_aggregate_used(self)?;
        }
        Ok(&self.within_group)
    }
}

#[cfg(test)]
mod test_within_group {
    use crate::syn::sqlite::expr::function_call::Expr;
    use crate::syn::sqlite::{ExprFunctionCall, NullsOrder, SortOrder};
    use crate::test_helpers::SqliteSynBuilder;
    use crate::{engine_specific::TursoOrderedSetAggregateFeature, sem::AnalysisError};

    #[derive(Default)]
    struct Gate(usize);
    impl TursoOrderedSetAggregateFeature for Gate {
        fn ordered_set_aggregate_used(
            &mut self,
            _: &ExprFunctionCall,
        ) -> Result<(), AnalysisError> {
            self.0 += 1;
            Ok(())
        }
    }

    #[test]
    fn preserves_ordered_set_and_argument_ordering_separately() {
        let Expr::FunctionCall(call) = SqliteSynBuilder.parse_expression(
            "SELECT f(DISTINCT x ORDER BY y) WITHIN GROUP (ORDER BY z DESC NULLS FIRST, w ASC NULLS LAST) FILTER (WHERE x > 0) OVER ()"
        ) else { panic!("expected function call") };
        assert_eq!(call.name().value(), "f");
        assert!(call.distinctness().is_some());
        assert_eq!(call.args().len(), 1);
        assert_eq!(call.order_by().len(), 1);
        assert!(call.filter_over().filter_clause().is_some());
        assert!(call.filter_over().over_clause().is_some());
        let mut gate = Gate::default();
        let ordering = call.within_group(&mut gate).unwrap();
        assert_eq!(gate.0, 1);
        assert_eq!(ordering.len(), 2);
        assert!(matches!(ordering[0].order(), Some(SortOrder::Desc)));
        assert!(matches!(ordering[0].nulls(), Some(NullsOrder::First)));
        assert!(matches!(ordering[1].order(), Some(SortOrder::Asc)));
        assert!(matches!(ordering[1].nulls(), Some(NullsOrder::Last)));
        let Expr::Id(name) = ordering[0].expr().as_ref() else {
            panic!("expected sort key")
        };
        assert_eq!(name.value(), "z");
    }

    struct Reject;
    impl TursoOrderedSetAggregateFeature for Reject {
        fn ordered_set_aggregate_used(
            &mut self,
            _: &ExprFunctionCall,
        ) -> Result<(), AnalysisError> {
            Err(std::io::Error::other("ordered-set aggregate denied").into())
        }
    }

    #[test]
    fn propagates_engine_hook_errors_after_parsing() {
        let Expr::FunctionCall(call) =
            SqliteSynBuilder.parse_expression("SELECT f(x) WITHIN GROUP (ORDER BY y)")
        else {
            panic!("expected function call");
        };
        assert_eq!(
            call.within_group(&mut Reject).unwrap_err().to_string(),
            "ordered-set aggregate denied"
        );
    }

    #[test]
    fn ordinary_functions_do_not_notify_turso_hook() {
        let Expr::FunctionCall(call) = SqliteSynBuilder.parse_expression("SELECT f(x ORDER BY y)")
        else {
            panic!("expected function call")
        };
        let mut gate = Gate::default();
        assert!(call.within_group(&mut gate).unwrap().is_empty());
        assert_eq!(gate.0, 0);
        assert_eq!(call.order_by().len(), 1);
    }
}

impl Expr {
    /// create FunctionCall variant
    pub fn function_call<NAME, DISTINCT, ARG, SORT, TAIL>(
        name: NAME,
        distinctness: Option<DISTINCT>,
        args: Vec<Box<ARG>>,
        order_by: Vec<SORT>,
        within_group: Vec<SORT>,
        filter_over: TAIL,
    ) -> Self
    where
        NAME: Into<Name>,
        DISTINCT: Into<Distinctness>,
        ARG: Into<Self>,
        SORT: Into<SortedColumn>,
        TAIL: Into<FunctionTail>,
    {
        Self::FunctionCall(ExprFunctionCall::new(
            &name.into(),
            &distinctness.map(|v| v.into()),
            &args.into_iter().map(|v| (*v).into()).collect::<Vec<_>>(),
            &order_by.into_iter().map(|v| v.into()).collect::<Vec<_>>(),
            &within_group
                .into_iter()
                .map(|v| v.into())
                .collect::<Vec<_>>(),
            &filter_over.into(),
        ))
    }
}

/// function call star expression
#[derive(Clone, Debug, derive_getters::Getters)]
pub struct ExprFunctionCallStar {
    name: Name,
    filter_over: FunctionTail,
}

impl ExprFunctionCallStar {
    /// create instance
    pub const fn new(name: Name, filter_over: FunctionTail) -> Self {
        Self { name, filter_over }
    }
}

impl Expr {
    /// create FunctionCallStar variant
    pub fn function_call_star<NAME, TAIL>(name: NAME, filter_over: TAIL) -> Self
    where
        NAME: Into<Name>,
        TAIL: Into<FunctionTail>,
    {
        Self::FunctionCallStar(ExprFunctionCallStar::new(name.into(), filter_over.into()))
    }
}
