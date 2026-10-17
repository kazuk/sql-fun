mod query;

use crate::StringSpan;
use crate::ast::{
    Access, BooleanExpr, BooleanOperator, Call, CallKind, Case, Cast, Dialect, Expr, Indirection,
    Literal, Name, NullTest, OperatorExpr, OperatorKind, SqlValue, Subquery, TypeName,
};
use crate::sem::{
    AliasName, AnalysisError, AnalysisProblem, ArithExpr, CaseExpr, CaseExprAnalysisContext,
    CoalesceAnalysis, CoalesceExpr, ColumnName, FullName, FuncCallExpr, IndirectionExpr,
    OperatorName, ParseContext, ScalarAnalysisContext, ScalarConstExpr, SemScalarExpr,
    SqlValueExpr, TypeReference,
};

impl Name {
    pub(super) fn full_name(&self) -> Result<FullName, AnalysisError> {
        match self.0.as_slice() {
            [local] => Ok(FullName::from_local_name(local)),
            [schema, local] => Ok(FullName::with_schema(schema, local)),
            [catalog, schema, local] => Ok(FullName::with_catalog(catalog, Some(schema), local)),
            _ => AnalysisError::raise_unexpected_input("qualified name component count"),
        }
    }

    fn operator_name(&self) -> Result<OperatorName, AnalysisError> {
        match self.0.as_slice() {
            [local] => Ok(OperatorName::from_parts(None, local)),
            [schema, local] => Ok(OperatorName::from_parts(Some(schema), local)),
            _ => AnalysisError::raise_unexpected_input("operator name component count"),
        }
    }

    fn resolve_column(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let (alias, column) = match self.0.as_slice() {
            [column] => (None, column),
            [alias, column] => (Some(AliasName::from(alias.as_str())), column),
            _ => return AnalysisError::raise_unexpected_input("column qualification"),
        };
        let value = scope
            .resolve_column(
                &mut *context,
                alias.as_ref(),
                &ColumnName::from(column.as_str()),
            )?
            .0;
        Ok(value)
    }
}

impl Expr {
    fn negated_literal(&self) -> Option<SemScalarExpr> {
        match self {
            Self::Literal(literal) => literal.negate(),
            _ => None,
        }
    }
    pub(crate) fn resolve(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemScalarExpr, crate::sem::AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = self.resolve_in(context, scope);
        let result = context.infer_expression_parameter_types(result);
        context.finish_analysis(result)
    }

    pub(super) fn resolve_in(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        crate::sem::with_stack(|| self.resolve_in_body(context, scope))
    }

    #[inline(never)]
    fn resolve_in_body(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        match self {
            Self::Parameter(parameter) => Ok(context.parameter(parameter.position)),
            Self::Literal(value) => Ok(value.resolve()),
            Self::Column(name) => name.resolve_column(context, scope),
            // The legacy column adapter discarded the star component. Preserve
            // its binding behavior here; projection expansion is a separate task.
            Self::Star(name) => name.resolve_column(context, scope),
            Self::Operator(operator) => operator.resolve(context, scope),
            Self::Boolean(boolean) => boolean.resolve(context, scope),
            Self::NullTest(test) => test.resolve(context, scope),
            Self::Cast(cast) => cast.resolve(context, scope),
            Self::Call(call) => call.resolve(context, scope),
            Self::Case(case) => case.resolve(context, scope),
            Self::Subquery(query) => query.resolve(context, scope),
            Self::Array(items) => Ok(SemScalarExpr::array(&Self::resolve_list(
                items, context, scope,
            )?)),
            Self::List(items) => Ok(SemScalarExpr::list(&Self::resolve_list(
                items, context, scope,
            )?)),
            Self::Parenthesized(items) => match items.as_slice() {
                [expression] => expression.resolve_in(context, scope),
                _ => Ok(SemScalarExpr::list(&Self::resolve_list(
                    items, context, scope,
                )?)),
            },
            Self::Indirection(indirection) => indirection.resolve(context, scope),
            Self::SqlValue(value) => value.resolve(),
            Self::Collate(collate) if context.dialect() == Dialect::sqlite() => {
                collate.argument.resolve_in(context, scope)
            }
            // Retain the legacy rejection of raw PostgreSQL syntax that was not
            // accepted by the scalar dispatcher, independently of its conversion.
            Self::Collate(_)
            | Self::Document(_)
            | Self::Grouping(_)
            | Self::Row(_)
            | Self::CurrentOf(_)
            | Self::MergeAction
            | Self::TruthTest(_) => {
                AnalysisError::raise_unexpected_input("unsupported raw scalar expression")
            }
            Self::Default if context.dialect() == Dialect::postgres() => {
                AnalysisError::raise_unexpected_input("unsupported raw scalar expression")
            }
            // Preserve existing explicit unimplemented markers as well.
            Self::NamedArgument(_) | Self::Default | Self::Raise(_) | Self::InTable(_) => {
                todo!("expression semantic analysis")
            }
        }
    }

    fn resolve_list(
        expressions: &[Self],
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Vec<SemScalarExpr>, AnalysisError> {
        let mut values = Vec::new();
        for expression in expressions {
            values.push(expression.resolve_in(context, scope)?);
        }
        Ok(values)
    }

    fn resolve_optional(
        expression: &Option<Self>,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Option<SemScalarExpr>, AnalysisError> {
        expression
            .as_ref()
            .map(|expression| expression.resolve_in(context, scope))
            .transpose()
    }
}

impl Literal {
    fn resolve(&self) -> SemScalarExpr {
        let value = match self {
            Self::Null => return ScalarConstExpr::null(),
            Self::Integer(value) => ScalarConstExpr::new_integer(*value),
            Self::Numeric(value) => ScalarConstExpr::numeric(value),
            Self::String(value) => ScalarConstExpr::string(value),
            Self::Boolean(value) => ScalarConstExpr::boolean(*value),
            Self::BitString(value) => ScalarConstExpr::bit_string(value),
            Self::Blob(_) | Self::Keyword(_) => todo!("literal semantic analysis"),
        };
        SemScalarExpr::literal(&value)
    }

    fn negate(&self) -> Option<SemScalarExpr> {
        match self {
            Self::Integer(value) => value
                .checked_neg()
                .map(|value| SemScalarExpr::literal(&ScalarConstExpr::new_integer(value))),
            Self::Numeric(value) => Some(SemScalarExpr::literal(&ScalarConstExpr::numeric(
                &format!("-{value}"),
            ))),
            _ => None,
        }
    }
}

impl BooleanExpr {
    fn resolve(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let arguments = Expr::resolve_list(&self.arguments, context, scope)?;
        self.operator.resolve(&arguments)
    }
}

impl NullTest {
    fn resolve(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let argument = self.argument.resolve_in(context, scope)?;
        Ok(SemScalarExpr::null_test(&argument, self.negated))
    }
}

impl BooleanOperator {
    fn resolve(&self, arguments: &[SemScalarExpr]) -> Result<SemScalarExpr, AnalysisError> {
        match self {
            Self::And => Ok(SemScalarExpr::logical_and(arguments)),
            Self::Or => Ok(SemScalarExpr::logical_or(arguments)),
            Self::Not => match arguments {
                [value] => Ok(SemScalarExpr::logical_not(value)),
                _ => AnalysisError::raise_unexpected_input("NOT argument count"),
            },
        }
    }
}

impl OperatorExpr {
    fn resolve(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        if context.dialect() == Dialect::sqlite() && self.left.is_none() {
            if self.name.0.as_slice() == ["+"] {
                return self
                    .right
                    .as_ref()
                    .expect("unary argument")
                    .resolve_in(context, scope);
            }
            if self.name.0.as_slice() == ["-"]
                && let Some(right) = self.right.as_ref()
                && let Some(value) = right.negated_literal()
            {
                return Ok(value);
            }
        }
        if context.dialect() == Dialect::sqlite()
            && self.right.as_ref().is_some_and(Expr::is_null_literal)
            && (self.name.0.as_slice() == ["is"] || self.name.0.as_slice() == ["is not"])
        {
            let argument = self
                .left
                .as_ref()
                .expect("IS left operand")
                .resolve_in(context, scope)?;
            let negated = self.name.0.as_slice() == ["is not"];
            return Ok(SemScalarExpr::null_test(&argument, negated));
        }
        let left = Expr::resolve_optional(&self.left, context, scope)?;
        let right = Expr::resolve_optional(&self.right, context, scope)?;
        if context.dialect() == Dialect::sqlite() && !self.kind.without_catalog_operator() {
            // SQLite coercion/affinity is not PostgreSQL operator overload lookup.
            // This was unimplemented by the SQLite analyzer before the migration.
            if left
                .as_ref()
                .is_some_and(|value| value.get_type().is_none())
                || right
                    .as_ref()
                    .is_some_and(|value| value.get_type().is_none())
            {
                return Ok(ArithExpr::deferred_operator(
                    &self.name.operator_name()?,
                    left,
                    right,
                ));
            }
            todo!("SQLite operator semantics");
        }
        let binding = self.kind.resolve_operator_name(
            context,
            &self.name,
            left.is_some(),
            right.is_some(),
        )?;
        let mut expression = self.kind.resolve(context, &self.name, left, right)?;
        if let Some(binding) = binding {
            expression.set_operator_binding(binding);
        }
        Ok(expression)
    }
}

impl OperatorKind {
    fn resolve_operator_name(
        &self,
        context: &impl ParseContext,
        name: &Name,
        left: bool,
        right: bool,
    ) -> Result<Option<crate::sem::ResolvedOperator>, AnalysisError> {
        match self {
            Self::Ordinary | Self::Any | Self::All => Ok(Some(
                crate::sem::ResolvedOperator::resolve(context, &name.operator_name()?, left, right),
            )),
            _ => Ok(None),
        }
    }

    const fn without_catalog_operator(&self) -> bool {
        matches!(self, Self::Between | Self::NotBetween)
    }
    fn resolve(
        &self,
        context: &mut impl ParseContext,
        name: &Name,
        left: Option<SemScalarExpr>,
        right: Option<SemScalarExpr>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        match self {
            Self::Ordinary => {
                Ok(ArithExpr::build_op(context, &name.operator_name()?, left, right)?.0)
            }
            Self::Any => {
                Ok(
                    ArithExpr::build_op_any_resolved(context, &name.operator_name()?, left, right)?
                        .0,
                )
            }
            Self::All => {
                Ok(
                    ArithExpr::build_op_all_resolved(context, &name.operator_name()?, left, right)?
                        .0,
                )
            }
            Self::NullIf => Ok(ArithExpr::build_null_if(context, left, right)?.0),
            Self::Between | Self::NotBetween => self.resolve_between(left, right),
            _ => todo!("operator semantic analysis"),
        }
    }
    fn resolve_between(
        &self,
        left: Option<SemScalarExpr>,
        right: Option<SemScalarExpr>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let left = left.expect("BETWEEN argument");
        let values = right.expect("BETWEEN bounds").into_list_values()?;
        let Ok([start, end]): Result<[SemScalarExpr; 2], _> = values.try_into() else {
            return AnalysisError::raise_unexpected_input("BETWEEN bounds");
        };
        Ok(if matches!(self, Self::NotBetween) {
            ArithExpr::not_between(left, (start, end))
        } else {
            ArithExpr::between(left, (start, end))
        })
    }
}

impl TypeName {
    fn reference(&self) -> Result<TypeReference, AnalysisError> {
        Ok(TypeReference::from_full_name(self.name.full_name()?))
    }
}

impl Cast {
    fn resolve(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let target = self.target.reference()?;
        let argument = self.argument.resolve_in(context, scope)?;
        let mut resolution = crate::sem::QueryNameResolution::affinity(&target.to_string());
        let target = if context.dialect() == Dialect::postgres() {
            if let Some(canonical) = context.canonicalize_type_ref(&target) {
                resolution = crate::sem::QueryNameResolution::type_reference(&canonical);
                canonical
            } else {
                resolution = crate::sem::QueryNameResolution::not_found();
                context.report_analysis_problem(AnalysisProblem::parameter_constraint(
                    &format!("cast target {target} not found"),
                ))?;
                target
            }
        } else {
            target
        };
        if context.dialect() == Dialect::postgres()
            && let Some(source) = argument.get_type()
            && context.get_explicit_cast(&source, &target).is_none()
        {
            context.report_analysis_problem(AnalysisProblem::explicit_cast_not_defined(
                &source, &target,
            ))?;
        }
        let mut expression = SemScalarExpr::explicit_cast(&argument, &target);
        expression.set_cast_resolution(resolution);
        Ok(expression)
    }
}

impl Call {
    fn resolve(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let mut expression = self.kind.resolve(self, context, scope)?;
        let modifiers = self.resolve_modifiers(context, scope)?;
        expression.set_function_modifiers(modifiers);
        Ok(expression)
    }
}

impl CallKind {
    fn resolve(
        &self,
        call: &Call,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        if context.dialect() == Dialect::sqlite() {
            todo!("SQLite function affinity and overload semantics");
        }
        match self {
            Self::Coalesce => call.resolve_coalesce(context, scope),
            Self::MinMax => todo!("min/max semantic analysis"),
            Self::Ordinary => call.resolve_ordinary(context, scope),
        }
    }
}

impl Call {
    fn resolve_coalesce(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let arguments = Expr::resolve_list(&self.arguments, context, scope)?;
        if let Some(expression) = CoalesceExpr::defer_untyped(&arguments) {
            return Ok(expression);
        }
        let mut analysis = CoalesceAnalysis::new();
        for argument in &arguments {
            analysis.push(context, argument)?;
        }
        Ok(analysis.finish(&mut *context)?.0)
    }

    fn resolve_ordinary(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let arguments = Expr::resolve_list(&self.arguments, context, scope)?;
        FuncCallExpr::resolve_common(
            context,
            &self.name.full_name()?,
            &arguments,
            &self.location.unwrap_or_else(|| StringSpan::new(0, 0)),
        )
    }
}

impl Case {
    fn resolve(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let argument = Expr::resolve_optional(&self.argument, context, scope)?;
        let mut arms = Vec::new();
        for arm in &self.arms {
            let condition = arm.condition.resolve_in(context, scope)?;
            let result = arm.result.resolve_in(context, scope)?;
            arms.push((condition, result));
        }
        let otherwise = Expr::resolve_optional(&self.otherwise, context, scope)?;
        if let Some(expression) = CaseExpr::defer_untyped(&argument, &arms, &otherwise) {
            return Ok(expression);
        }
        let mut analysis = if context.dialect() == Dialect::sqlite() {
            CaseExprAnalysisContext::without_catalog_casts()
        } else {
            CaseExprAnalysisContext::new()
        };
        analysis.set_argument(&argument);
        for (condition, result) in &arms {
            analysis.push_arm(context, condition, result)?;
        }
        analysis.set_default(context, &otherwise)?;
        analysis.finish(&mut *context)
    }
}

impl Subquery {
    fn resolve(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let query = self.query.resolve_in(context, Some(scope))?;
        SemScalarExpr::subquery(&query)
    }
}

impl SqlValue {
    fn resolve(&self) -> Result<SemScalarExpr, AnalysisError> {
        SqlValueExpr::from_common(&self.name, self.precision)
    }
}

impl Indirection {
    fn resolve(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let mut value = self.argument.resolve_in(context, scope)?;
        for item in &self.items {
            value = item.resolve(context, scope, value)?;
        }
        Ok(value)
    }
}

impl Access {
    fn resolve(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        base: SemScalarExpr,
    ) -> Result<SemScalarExpr, AnalysisError> {
        match self {
            Self::Field(name) => IndirectionExpr::resolved_member(context, base, name),
            Self::Index(index) => {
                let index = index.resolve_in(context, scope)?;
                IndirectionExpr::resolved_index(context, base, index)
            }
            Self::Slice(slice) => {
                let lower = Expr::resolve_optional(&slice.lower, context, scope)?;
                let upper = Expr::resolve_optional(&slice.upper, context, scope)?;
                IndirectionExpr::resolved_slice(context, base, lower, upper)
            }
            Self::Star => AnalysisError::raise_unexpected_input("indirection star"),
        }
    }
}
