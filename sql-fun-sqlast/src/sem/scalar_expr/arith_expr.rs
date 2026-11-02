mod between;
mod binary;
mod call_sites;
mod impl_build_op;
mod null_if;
mod pattern;
mod sqlite_like;
mod unary;

pub use self::{
    call_sites::{
        AllBinaryCallSite, AnyBinaryCallSite, BinaryOpCallSite, LeftUnaryCallSite,
        RightUnaryCallSite, UndefinedOperatorAllCallSite, UndefinedOperatorAnyCallSite,
        UndefinedOperatorCallSite,
    },
    null_if::NullIfExpr,
    pattern::PatternExpr,
    sqlite_like::SqliteLikeExpr,
};

use crate::sem::{SemScalarExpr, TypeReference, scalar_expr::SemScalarExprNode};

/// arith expression
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ArithExpr {
    /// binary operator expression
    OpBinary(Box<BinaryOpCallSite>),
    /// left unary expression
    OpLeftUnary(Box<LeftUnaryCallSite>),
    /// right unary expression
    OpRightUnary(Box<RightUnaryCallSite>),
    /// any binary expression
    OpAnyBinary(Box<AnyBinaryCallSite>),
    /// all binary expression
    OpAllBinary(Box<AllBinaryCallSite>),

    /// undefined operator
    UndefinedOperator(Box<UndefinedOperatorCallSite>),
    /// undefined operator Any
    UndefinedOperatorAny(Box<UndefinedOperatorAnyCallSite>),
    /// undefined operator All
    UndefinedOperatorAll(Box<UndefinedOperatorAllCallSite>),

    /// null-if  expression
    NullIfExpr(Box<NullIfExpr>),

    /// Deferred PostgreSQL pattern call with unknown literal operands.
    Pattern(Box<PatternExpr>),

    /// SQLite builtin LIKE predicate, without a PostgreSQL operator binding.
    SqliteLike(Box<SqliteLikeExpr>),

    /// Between
    Between(Box<BetweenExpr>),
}

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct BetweenExpr {
    left: SemScalarExpr,
    start: SemScalarExpr,
    end: SemScalarExpr,
    not: bool,
}

impl ArithExpr {
    pub(super) fn canonicalize_grouping_key(&mut self) {
        if let Some(binding) = self.binding() {
            let mut binding = binding.clone();
            binding.canonicalize_grouping_key();
            self.set_binding(binding);
        }
    }

    /// Build a context-free BETWEEN expression.
    ///
    /// PostgreSQL semantic analysis resolves its comparisons before parameter inference.
    /// Until then, its result type is boolean and its nullability is unknown.
    pub fn between(left: SemScalarExpr, range: (SemScalarExpr, SemScalarExpr)) -> SemScalarExpr {
        SemScalarExpr::Arith(Self::Between(Box::new(BetweenExpr {
            left,
            start: range.0,
            end: range.1,
            not: false,
        })))
    }

    /// Build a context-free NOT BETWEEN expression.
    ///
    /// PostgreSQL semantic analysis resolves its comparisons before parameter inference.
    /// Until then, its result type is boolean and its nullability is unknown.
    pub fn not_between(
        left: SemScalarExpr,
        range: (SemScalarExpr, SemScalarExpr),
    ) -> SemScalarExpr {
        SemScalarExpr::Arith(Self::Between(Box::new(BetweenExpr {
            left,
            start: range.0,
            end: range.1,
            not: true,
        })))
    }

    fn inputs_is_not_null(&self) -> Option<bool> {
        match &self {
            Self::OpLeftUnary(call_site) => call_site.is_not_null(),
            Self::OpRightUnary(call_site) => call_site.is_not_null(),
            Self::UndefinedOperator(call_site) => call_site.inputs_is_not_null(),
            Self::OpBinary(call_site) => call_site.is_not_null(),
            Self::OpAnyBinary(call_site) => call_site.is_not_null(),
            Self::OpAllBinary(call_site) => call_site.is_not_null(),
            Self::UndefinedOperatorAny(call_site) => call_site.is_not_null(),
            Self::UndefinedOperatorAll(call_site) => call_site.is_not_null(),
            Self::NullIfExpr(null_if) => null_if.is_not_null(),
            Self::Between(value) => value.inputs_is_not_null(),
            Self::Pattern(_) => None,
            Self::SqliteLike(value) => value.is_not_null(),
        }
    }

    const fn operator_returns_null(&self) -> Option<bool> {
        None
    }
}

impl SemScalarExprNode for ArithExpr {
    fn get_type(&self) -> Option<TypeReference> {
        match &self {
            Self::OpBinary(call_site) => Some(call_site.get_return_type().clone()),
            Self::OpLeftUnary(call_site) => Some(call_site.get_return_type().clone()),
            Self::OpRightUnary(call_site) => Some(call_site.get_return_type().clone()),
            Self::OpAnyBinary(call_site) => Some(call_site.get_return_type().clone()),
            Self::OpAllBinary(call_site) => Some(call_site.get_return_type().clone()),
            Self::NullIfExpr(null_if) => null_if.get_type(),
            Self::UndefinedOperator(_)
            | Self::UndefinedOperatorAny(_)
            | Self::UndefinedOperatorAll(_) => None,
            Self::Between(_) => Some(crate::sem::PgBuiltInType::bool()),
            Self::Pattern(_) => None,
            Self::SqliteLike(value) => value.get_type(),
        }
    }

    fn is_not_null(&self) -> Option<bool> {
        let is_strict = match &self {
            Self::OpLeftUnary(site) => site.is_strict(),
            Self::OpRightUnary(site) => site.is_strict(),
            Self::OpBinary(call_site) => call_site.is_strict(),
            Self::OpAnyBinary(call_site) => return call_site.is_not_null(),
            Self::OpAllBinary(call_site) => return call_site.is_not_null(),
            Self::NullIfExpr(null_if) => null_if.is_strict(),
            Self::UndefinedOperator(_)
            | Self::UndefinedOperatorAny(_)
            | Self::UndefinedOperatorAll(_) => return None,
            // A context-free builder has no selected comparison signatures yet.
            // Analysis expands it before collecting operand constraints.
            Self::Between(_) => return None,
            Self::Pattern(_) => return None,
            Self::SqliteLike(value) => return value.is_not_null(),
        };
        if is_strict {
            if matches!(self.inputs_is_not_null(), Some(true)) {
                if matches!(self.operator_returns_null(), Some(false)) {
                    Some(true)
                } else {
                    Some(false)
                }
            } else {
                Some(false)
            }
        } else {
            None
        }
    }
}

impl BetweenExpr {
    pub(crate) fn walk<V: crate::sem::traversal::SemanticVisitorMut>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), crate::sem::AnalysisError> {
        self.left.walk(visitor)?;
        self.start.walk(visitor)?;
        self.end.walk(visitor)?;
        Ok(())
    }
    pub(crate) fn visit<V: crate::sem::traversal::SemanticVisitor>(
        &self,
        visitor: &mut V,
    ) -> Result<(), crate::sem::AnalysisError> {
        self.left.visit(visitor)?;
        self.start.visit(visitor)?;
        self.end.visit(visitor)?;
        Ok(())
    }
}

impl ArithExpr {
    pub(crate) fn walk<V: crate::sem::traversal::SemanticVisitorMut>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), crate::sem::AnalysisError> {
        match self {
            Self::OpBinary(v) => v.walk(visitor)?,
            Self::OpLeftUnary(v) => v.walk(visitor)?,
            Self::OpRightUnary(v) => v.walk(visitor)?,
            Self::OpAnyBinary(v) => v.walk(visitor)?,
            Self::OpAllBinary(v) => v.walk(visitor)?,
            Self::UndefinedOperator(v) => v.walk(visitor)?,
            Self::UndefinedOperatorAny(v) => v.walk(visitor)?,
            Self::UndefinedOperatorAll(v) => v.walk(visitor)?,
            Self::NullIfExpr(v) => v.walk(visitor)?,
            Self::Between(v) => v.walk(visitor)?,
            Self::Pattern(value) => value.walk(visitor)?,
            Self::SqliteLike(value) => value.walk(visitor)?,
        }
        Ok(())
    }
    pub(crate) fn visit<V: crate::sem::traversal::SemanticVisitor>(
        &self,
        visitor: &mut V,
    ) -> Result<(), crate::sem::AnalysisError> {
        match self {
            Self::OpBinary(v) => v.visit(visitor)?,
            Self::OpLeftUnary(v) => v.visit(visitor)?,
            Self::OpRightUnary(v) => v.visit(visitor)?,
            Self::OpAnyBinary(v) => v.visit(visitor)?,
            Self::OpAllBinary(v) => v.visit(visitor)?,
            Self::UndefinedOperator(v) => v.visit(visitor)?,
            Self::UndefinedOperatorAny(v) => v.visit(visitor)?,
            Self::UndefinedOperatorAll(v) => v.visit(visitor)?,
            Self::NullIfExpr(v) => v.visit(visitor)?,
            Self::Between(v) => v.visit(visitor)?,
            Self::Pattern(value) => value.visit(visitor)?,
            Self::SqliteLike(value) => value.visit(visitor)?,
        }
        Ok(())
    }
}

impl BetweenExpr {
    fn collect_constraints(
        &self,
        parameters: &mut crate::sem::parameter_resolution::ParameterResolution<'_>,
    ) {
        if self.left.constraint_node().is_some()
            || self.start.constraint_node().is_some()
            || self.end.constraint_node().is_some()
        {
            parameters.special_operator("BETWEEN", &[&self.left, &self.start, &self.end]);
        }
    }
}
impl ArithExpr {
    pub(crate) fn collect_constraints(
        &self,
        parameters: &mut crate::sem::parameter_resolution::ParameterResolution<'_>,
        context: &mut impl crate::sem::ParseContext,
    ) -> Result<Option<SemScalarExpr>, crate::sem::AnalysisError> {
        match self {
            Self::Pattern(value) => value.collect_constraints(parameters, context),
            Self::OpBinary(v) => v.collect_constraints(parameters, context),
            Self::OpLeftUnary(v) => v.collect_constraints(parameters, context),
            Self::OpRightUnary(v) => v.collect_constraints(parameters, context),
            Self::UndefinedOperator(v) => v.collect_constraints(parameters, context),
            Self::OpAnyBinary(v) => {
                v.collect_constraints(parameters);
                Ok(None)
            }
            Self::OpAllBinary(v) => {
                v.collect_constraints(parameters);
                Ok(None)
            }
            Self::UndefinedOperatorAny(v) => {
                v.collect_constraints(parameters);
                Ok(None)
            }
            Self::UndefinedOperatorAll(v) => {
                v.collect_constraints(parameters);
                Ok(None)
            }
            Self::NullIfExpr(v) => {
                v.collect_constraints(parameters);
                Ok(None)
            }
            Self::Between(v) => {
                v.collect_constraints(parameters);
                Ok(None)
            }
            // Traversal collects operand constraints; SQLite LIKE accepts any affinity
            // and its integer result imposes no operand type relationship.
            Self::SqliteLike(_) => Ok(None),
        }
    }
}

impl ArithExpr {
    pub(crate) fn deferred_operator(
        name: &crate::sem::OperatorName,
        left: Option<SemScalarExpr>,
        right: Option<SemScalarExpr>,
    ) -> SemScalarExpr {
        let call = UndefinedOperatorCallSite::from_inputs(name, left, right);
        SemScalarExpr::arithmetic(Self::UndefinedOperator(Box::new(call)))
    }
}

impl ArithExpr {
    pub(crate) fn deferred_any_operator(
        name: &crate::sem::OperatorName,
        left: SemScalarExpr,
        right: SemScalarExpr,
    ) -> SemScalarExpr {
        let call = UndefinedOperatorAnyCallSite::from_inputs(name, left, right);
        SemScalarExpr::arithmetic(Self::UndefinedOperatorAny(Box::new(call)))
    }
}

impl ArithExpr {
    pub(crate) fn deferred_all_operator(
        name: &crate::sem::OperatorName,
        left: SemScalarExpr,
        right: SemScalarExpr,
    ) -> SemScalarExpr {
        let call = UndefinedOperatorAllCallSite::from_inputs(name, left, right);
        SemScalarExpr::arithmetic(Self::UndefinedOperatorAll(Box::new(call)))
    }
}

impl ArithExpr {
    pub(crate) fn binary_argument(&self, index: usize) -> Option<&SemScalarExpr> {
        match self {
            Self::OpBinary(value) => value.argument(index),
            Self::UndefinedOperator(value) => value.argument(index),
            _ => None,
        }
    }
    pub(crate) fn set_binding(&mut self, binding: crate::sem::ResolvedOperator) {
        match self {
            Self::Pattern(value) => value.set_binding(binding),
            Self::OpBinary(call) => call.set_binding(binding),
            Self::OpLeftUnary(call) => call.set_binding(binding),
            Self::OpRightUnary(call) => call.set_binding(binding),
            Self::OpAnyBinary(call) => call.set_binding(binding),
            Self::OpAllBinary(call) => call.set_binding(binding),
            Self::UndefinedOperator(call) => call.set_binding(binding),
            Self::UndefinedOperatorAny(call) => call.set_binding(binding),
            Self::UndefinedOperatorAll(call) => call.set_binding(binding),
            _ => {}
        }
    }
    /// Operator-name resolution, retained separately from overload/type selection.
    pub fn binding(&self) -> Option<&crate::sem::ResolvedOperator> {
        match self {
            Self::Pattern(value) => Some(value.binding()),
            Self::OpBinary(call) => call.binding(),
            Self::OpLeftUnary(call) => call.binding(),
            Self::OpRightUnary(call) => call.binding(),
            Self::OpAnyBinary(call) => call.binding(),
            Self::OpAllBinary(call) => call.binding(),
            Self::UndefinedOperator(call) => call.binding(),
            Self::UndefinedOperatorAny(call) => call.binding(),
            Self::UndefinedOperatorAll(call) => call.binding(),
            _ => None,
        }
    }
}

impl ArithExpr {
    pub(crate) fn resolve_between(
        &self,
        context: &mut impl crate::sem::ParseContext,
    ) -> Result<Option<SemScalarExpr>, crate::sem::AnalysisError> {
        if context.dialect() == crate::ast::Dialect::postgres()
            && let Self::Between(value) = self
        {
            return Ok(Some(value.resolve(context)?));
        }
        Ok(None)
    }
}
