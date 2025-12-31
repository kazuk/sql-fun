use crate::sem::{
    BinaryOperatorDefinition, CastContext, ParseContext, SemScalarExpr, TypeReference,
};

pub struct BinaryCastCandiate {
    left: CastContext,
    right: CastContext,
    priority: i32,
    operator: BinaryOperatorDefinition,
}

impl BinaryCastCandiate {
    fn new(
        left: CastContext,
        right: CastContext,
        priority: i32,
        operator: BinaryOperatorDefinition,
    ) -> Self {
        Self {
            left,
            right,
            priority,
            operator,
        }
    }

    pub fn enumerate_from_context<TParseContext>(
        context: &TParseContext,
        overloads: &[BinaryOperatorDefinition],
        left_type: &TypeReference,
        right_type: &TypeReference,
    ) -> Vec<Self>
    where
        TParseContext: ParseContext,
    {
        let mut candidates = Vec::new();
        for overload in overloads {
            if let Some(left_cast) = context.get_implicit_cast(left_type, overload.left_arg_type())
                && let Some(right_cast) =
                    context.get_implicit_cast(right_type, overload.right_arg_type())
            {
                let priority = left_cast.context().overload_priority_score()
                    + right_cast.context().overload_priority_score();

                candidates.push(Self::new(
                    left_cast.context(),
                    right_cast.context(),
                    priority,
                    overload.clone(),
                ))
            }
        }
        candidates
    }

    pub fn wrap_implicit_cast(
        &self,
        lexpr: SemScalarExpr,
        rexpr: SemScalarExpr,
    ) -> (SemScalarExpr, SemScalarExpr) {
        let lexpr = if self.left.is_implicit() {
            SemScalarExpr::wrap_implicit_cast(self.operator.left_arg_type(), &lexpr)
        } else {
            lexpr
        };
        let rexpr = if self.right.is_implicit() {
            SemScalarExpr::wrap_implicit_cast(self.operator.right_arg_type(), &rexpr)
        } else {
            rexpr
        };
        (lexpr, rexpr)
    }

    pub fn filter_best_candiate(candiates: Vec<Self>) -> Vec<Self> {
        let priority = candiates
            .iter()
            .map(|v| v.priority)
            .max()
            .expect("get max priority");

        candiates
            .into_iter()
            .filter(|v| v.priority == priority)
            .collect()
    }

    pub fn operator(&self) -> &BinaryOperatorDefinition {
        &self.operator
    }
}
