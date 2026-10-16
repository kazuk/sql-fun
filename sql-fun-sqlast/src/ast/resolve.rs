mod parameters;
mod query;
pub(crate) use self::parameters::ParameterResolution;
pub(crate) use self::parameters::Signature;

use crate::StringSpan;
use crate::ast::{
    Access, BooleanExpr, BooleanOperator, Call, CallKind, Case, Cast, Dialect, Expr, Indirection,
    Literal, Name, NullTest, OperatorExpr, OperatorKind, SqlValue, Subquery, TypeName,
};
use crate::sem::{
    AliasName, AnalysisError, AnalysisProblem, ArithExpr, CaseExprAnalysisContext, Catalog,
    CoalesceAnalysis, ColumnName, FullName, FuncCallExpr, IndirectionExpr, OperatorName,
    Resolution, ResolutionContext, ResolutionOutcome, ScalarAnalysisContext, ScalarConstExpr,
    SemScalarExpr, SqlValueExpr, TypeReference,
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
        context: &mut Resolution<'_>,
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
        Ok(context.parameters().column(&value).unwrap_or(value))
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
        catalog: &dyn Catalog,
        dialect: Dialect,
        scope: &ScalarAnalysisContext<'_>,
    ) -> ResolutionOutcome<SemScalarExpr> {
        let mut context = Resolution::new(catalog);
        let result = self.resolve_with_parameters(&mut context, dialect, scope);
        context.finish(result)
    }

    fn resolve_with_parameters(
        &self,
        context: &mut Resolution<'_>,
        dialect: Dialect,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let value = self.resolve_in(context, dialect, scope)?;
        if context.solve_parameters()? {
            return self.resolve_in(context, dialect, scope);
        }
        Ok(value)
    }

    pub(super) fn resolve_in(
        &self,
        context: &mut Resolution<'_>,
        dialect: Dialect,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let previous = context.parameters().enter_expression(self);
        let result = crate::sem::with_stack(|| self.resolve_in_body(context, dialect, scope));
        context.parameters().leave_expression(previous);
        if let Ok(value) = &result {
            context.parameters().remember(self, value);
        }
        result
    }

    #[inline(never)]
    fn resolve_in_body(
        &self,
        context: &mut Resolution<'_>,
        dialect: Dialect,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        match self {
            Self::Parameter(parameter) => Ok(context.parameters().parameter(parameter.position)),
            Self::Literal(value) => Ok(value.resolve()),
            Self::Column(name) => name.resolve_column(context, scope),
            // The legacy column adapter discarded the star component. Preserve
            // its binding behavior here; projection expansion is a separate task.
            Self::Star(name) => name.resolve_column(context, scope),
            Self::Operator(operator) => operator.resolve(context, dialect, scope),
            Self::Boolean(boolean) => boolean.resolve(context, dialect, scope),
            Self::NullTest(test) => test.resolve(context, dialect, scope),
            Self::Cast(cast) => cast.resolve(context, dialect, scope),
            Self::Call(call) => call.resolve(context, dialect, scope),
            Self::Case(case) => case.resolve(context, dialect, scope),
            Self::Subquery(query) => query.resolve(context, dialect, scope),
            Self::Array(items) => Ok(SemScalarExpr::array(&Self::resolve_list(
                items, context, dialect, scope,
            )?)),
            Self::List(items) => Ok(SemScalarExpr::list(&Self::resolve_list(
                items, context, dialect, scope,
            )?)),
            Self::Parenthesized(items) => match items.as_slice() {
                [expression] => expression.resolve_in(context, dialect, scope),
                _ => Ok(SemScalarExpr::list(&Self::resolve_list(
                    items, context, dialect, scope,
                )?)),
            },
            Self::Indirection(indirection) => indirection.resolve(context, dialect, scope),
            Self::SqlValue(value) => value.resolve(),
            Self::Collate(collate) if dialect == Dialect::sqlite() => {
                collate.argument.resolve_in(context, dialect, scope)
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
            Self::Default if dialect == Dialect::postgres() => {
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
        context: &mut Resolution<'_>,
        dialect: Dialect,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Vec<SemScalarExpr>, AnalysisError> {
        let mut values = Vec::new();
        for expression in expressions {
            values.push(expression.resolve_in(context, dialect, scope)?);
        }
        Ok(values)
    }

    fn resolve_optional(
        expression: &Option<Self>,
        context: &mut Resolution<'_>,
        dialect: Dialect,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Option<SemScalarExpr>, AnalysisError> {
        expression
            .as_ref()
            .map(|expression| expression.resolve_in(context, dialect, scope))
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
        context: &mut Resolution<'_>,
        dialect: Dialect,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let arguments = Expr::resolve_list(&self.arguments, context, dialect, scope)?;
        if arguments
            .iter()
            .any(|argument| argument.constraint_node().is_some())
        {
            if dialect == Dialect::postgres() {
                for argument in &arguments {
                    context.parameter_require(argument, &crate::sem::PgBuiltInType::bool(), false);
                }
            }
            return Ok(context.parameters().operation(
                "boolean",
                &arguments,
                &crate::sem::InputTypeConstraint::exact(&crate::sem::PgBuiltInType::bool()),
                None,
            ));
        }
        self.operator.resolve(&arguments)
    }
}

impl NullTest {
    fn resolve(
        &self,
        context: &mut Resolution<'_>,
        dialect: Dialect,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let argument = self.argument.resolve_in(context, dialect, scope)?;
        if argument.constraint_node().is_some() {
            return Ok(context.parameters().operation(
                if self.negated {
                    "IS NOT NULL"
                } else {
                    "IS NULL"
                },
                &[argument],
                &crate::sem::InputTypeConstraint::exact(&crate::sem::PgBuiltInType::bool()),
                Some(true),
            ));
        }
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
        context: &mut Resolution<'_>,
        dialect: Dialect,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        if dialect == Dialect::sqlite() && self.left.is_none() {
            if self.name.0.as_slice() == ["+"] {
                return self
                    .right
                    .as_ref()
                    .expect("unary argument")
                    .resolve_in(context, dialect, scope);
            }
            if self.name.0.as_slice() == ["-"]
                && let Some(value) = self.right.as_ref().and_then(Expr::negated_literal)
            {
                return Ok(value);
            }
        }
        if dialect == Dialect::sqlite()
            && self.right.as_ref().is_some_and(Expr::is_null_literal)
            && (self.name.0.as_slice() == ["is"] || self.name.0.as_slice() == ["is not"])
        {
            let argument = self
                .left
                .as_ref()
                .expect("IS left operand")
                .resolve_in(context, dialect, scope)?;
            let negated = self.name.0.as_slice() == ["is not"];
            if argument.constraint_node().is_some() {
                return Ok(context.parameters().operation(
                    if negated { "IS NOT NULL" } else { "IS NULL" },
                    &[argument],
                    &crate::sem::InputTypeConstraint::exact(&crate::sem::PgBuiltInType::bool()),
                    Some(true),
                ));
            }
            return Ok(SemScalarExpr::null_test(&argument, negated));
        }
        let left = Expr::resolve_optional(&self.left, context, dialect, scope)?;
        let right = Expr::resolve_optional(&self.right, context, dialect, scope)?;
        if dialect == Dialect::sqlite() && !self.kind.without_catalog_operator() {
            // SQLite coercion/affinity is not PostgreSQL operator overload lookup.
            // This was unimplemented by the SQLite analyzer before the migration.
            if left
                .as_ref()
                .is_some_and(|value| value.constraint_node().is_some())
                || right
                    .as_ref()
                    .is_some_and(|value| value.constraint_node().is_some())
            {
                todo!("SQLite operator type constraints");
            }
            todo!("SQLite operator semantics");
        }
        if left
            .as_ref()
            .is_some_and(|value| value.constraint_node().is_some())
            || right
                .as_ref()
                .is_some_and(|value| value.constraint_node().is_some())
        {
            return self
                .kind
                .resolve_parameter(context, &self.name, left.as_ref(), right.as_ref());
        }
        self.kind.resolve(context, &self.name, left, right)
    }
}

impl OperatorKind {
    const fn without_catalog_operator(&self) -> bool {
        matches!(self, Self::Between | Self::NotBetween)
    }
    fn resolve(
        &self,
        context: &mut Resolution<'_>,
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
        context: &mut Resolution<'_>,
        dialect: Dialect,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let target = self.target.reference()?;
        let argument = self.argument.resolve_in(context, dialect, scope)?;
        if argument.constraint_node().is_some() {
            let target = if dialect == Dialect::postgres() {
                let Some(canonical) = context.canonicalize_type_ref(&target) else {
                    return context.unresolved_parameter_operation(
                        &format!("cast target {target} not found"),
                        "CAST",
                        &[argument],
                        &crate::sem::InputTypeConstraint::exact(&target),
                    );
                };
                context.parameter_require(&argument, &canonical, true);
                canonical
            } else {
                target
            };
            return Ok(context.parameters().operation(
                "CAST",
                &[argument],
                &crate::sem::InputTypeConstraint::exact(&target),
                None,
            ));
        }
        #[cfg(feature = "sqlite")]
        if dialect == Dialect::sqlite() && context.get_type(target.full_name()).is_none() {
            context.report_problem(AnalysisProblem::type_not_found(target.full_name()))?;
        }
        if dialect == Dialect::postgres()
            && let Some(source) = argument.get_type()
            && context.get_explicit_cast(&source, &target).is_none()
        {
            context.report_problem(AnalysisProblem::explicit_cast_not_defined(&source, &target))?;
        }
        Ok(SemScalarExpr::explicit_cast(&argument, &target))
    }
}

impl Call {
    fn resolve(
        &self,
        context: &mut Resolution<'_>,
        dialect: Dialect,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        self.kind.resolve(self, context, dialect, scope)
    }
}

impl CallKind {
    fn resolve(
        &self,
        call: &Call,
        context: &mut Resolution<'_>,
        dialect: Dialect,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        if dialect == Dialect::sqlite() {
            todo!("SQLite function affinity and overload semantics");
        }
        match self {
            Self::Coalesce => call.resolve_coalesce(context, dialect, scope),
            Self::MinMax => todo!("min/max semantic analysis"),
            Self::Ordinary => call.resolve_ordinary(context, dialect, scope),
        }
    }
}

impl Call {
    fn resolve_coalesce(
        &self,
        context: &mut Resolution<'_>,
        dialect: Dialect,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let mut analysis = CoalesceAnalysis::new();
        for argument in &self.arguments {
            let value = argument.resolve_in(context, dialect, scope)?;
            if value.constraint_node().is_some() && value.get_type().is_none() {
                todo!("parameter-dependent COALESCE type unification");
            }
            analysis.push(context, &value)?;
        }
        Ok(analysis.finish(context)?.0)
    }

    fn resolve_ordinary(
        &self,
        context: &mut Resolution<'_>,
        dialect: Dialect,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let arguments = Expr::resolve_list(&self.arguments, context, dialect, scope)?;
        if arguments
            .iter()
            .any(|argument| argument.constraint_node().is_some())
        {
            let name = self.name.full_name()?;
            let Some(overloads) = context.get_function_by_name(&name) else {
                return context.unresolved_parameter_operation(
                    &format!("function {name} not found"),
                    &name.to_string(),
                    &arguments,
                    &crate::sem::InputTypeConstraint::any(),
                );
            };
            let signatures = context.parameter_function_signatures(&overloads, &arguments);
            return context.parameter_call(&name.to_string(), &arguments, &signatures);
        }
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
        context: &mut Resolution<'_>,
        dialect: Dialect,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let argument = Expr::resolve_optional(&self.argument, context, dialect, scope)?;
        let mut analysis = if dialect == Dialect::sqlite() {
            CaseExprAnalysisContext::without_catalog_casts()
        } else {
            CaseExprAnalysisContext::new()
        };
        if argument
            .as_ref()
            .is_some_and(|value| value.constraint_node().is_some() && value.get_type().is_none())
        {
            todo!("parameter-dependent CASE type unification");
        }
        analysis.set_argument(&argument);
        for arm in &self.arms {
            let condition = arm.condition.resolve_in(context, dialect, scope)?;
            let result = arm.result.resolve_in(context, dialect, scope)?;
            if (condition.constraint_node().is_some() && condition.get_type().is_none())
                || (result.constraint_node().is_some() && result.get_type().is_none())
            {
                todo!("parameter-dependent CASE type unification");
            }
            analysis.push_arm(context, &condition, &result)?;
        }
        let otherwise = Expr::resolve_optional(&self.otherwise, context, dialect, scope)?;
        if otherwise
            .as_ref()
            .is_some_and(|value| value.constraint_node().is_some() && value.get_type().is_none())
        {
            todo!("parameter-dependent CASE type unification");
        }
        analysis.set_default(context, &otherwise)?;
        analysis.finish(context)
    }
}

impl Subquery {
    fn resolve(
        &self,
        context: &mut Resolution<'_>,
        dialect: Dialect,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let query = self.query.resolve_in(context, dialect, Some(scope))?;
        let value = SemScalarExpr::subquery(&query)?;
        Ok(context.parameters().scalar_result(&value).unwrap_or(value))
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
        context: &mut Resolution<'_>,
        dialect: Dialect,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let mut value = self.argument.resolve_in(context, dialect, scope)?;
        if value.constraint_node().is_some() && value.get_type().is_none() {
            todo!("parameter-dependent indirection type constraints");
        }
        for item in &self.items {
            value = item.resolve(context, dialect, scope, value)?;
        }
        Ok(value)
    }
}

impl Access {
    fn resolve(
        &self,
        context: &mut Resolution<'_>,
        dialect: Dialect,
        scope: &ScalarAnalysisContext<'_>,
        base: SemScalarExpr,
    ) -> Result<SemScalarExpr, AnalysisError> {
        match self {
            Self::Field(name) => IndirectionExpr::resolved_member(context, base, name),
            Self::Index(index) => {
                let index = index.resolve_in(context, dialect, scope)?;
                IndirectionExpr::resolved_index(context, base, index)
            }
            Self::Slice(slice) => {
                let lower = Expr::resolve_optional(&slice.lower, context, dialect, scope)?;
                let upper = Expr::resolve_optional(&slice.upper, context, dialect, scope)?;
                IndirectionExpr::resolved_slice(context, base, lower, upper)
            }
            Self::Star => AnalysisError::raise_unexpected_input("indirection star"),
        }
    }
}

impl Expr {
    fn is_parameter_projection(&self) -> bool {
        match self {
            Self::Parameter(_) => true,
            Self::Parenthesized(expressions) => match expressions.as_slice() {
                [expression] => expression.is_parameter_projection(),
                _ => false,
            },
            _ => false,
        }
    }
}

impl OperatorKind {
    fn resolve_parameter(
        &self,
        context: &mut Resolution<'_>,
        name: &Name,
        left: Option<&SemScalarExpr>,
        right: Option<&SemScalarExpr>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        if !matches!(self, Self::Ordinary) {
            todo!("parameter constraints for this operator form");
        }
        let name = name.operator_name()?;
        let arguments: Vec<_> = left.into_iter().chain(right).cloned().collect();
        let Some(operator) = context.get_operator_op(&name, left.is_some(), right.is_some()) else {
            return context.unresolved_parameter_operation(
                &format!("operator {name:?} not found"),
                &format!("{name:?}"),
                &arguments,
                &crate::sem::InputTypeConstraint::any(),
            );
        };
        let signatures = operator.parameter_signatures();
        context.parameter_call(&format!("{name:?}"), &arguments, &signatures)
    }
}
