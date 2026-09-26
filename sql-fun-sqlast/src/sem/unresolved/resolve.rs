mod query;

use crate::StringSpan;
use crate::sem::unresolved::{
    Access, BooleanOperator, Call, CallKind, Case, Cast, Dialect, Expr, Indirection, Literal, Name,
    OperatorExpr, OperatorKind, SqlValue, Subquery, TypeName,
};
use crate::sem::{
    AnalysisError, AnalysisProblem, ArithExpr, ColumnName, FullName, FuncCallExpr, IndirectionExpr,
    OperatorName, ScalarAnalysisContext, ScalarConstExpr, SemScalarExpr, SqlValueExpr,
    TypeReference,
    data_source::AliasName,
    resolution::{Catalog, Resolution, ResolutionContext, ResolutionOutcome},
    scalar_expr::{CaseExprAnalysisContext, CoalesceAnalysis},
};

impl Name {
    fn full_name(&self) -> Result<FullName, AnalysisError> {
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
        Ok(scope
            .resolve_column(context, alias.as_ref(), &ColumnName::from(column.as_str()))?
            .0)
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
        let result = self.resolve_in(&mut context, dialect, scope);
        context.finish(result)
    }

    fn resolve_in(
        &self,
        context: &mut Resolution<'_>,
        dialect: Dialect,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        match self {
            Self::Literal(value) => Ok(value.resolve()),
            Self::Column(name) => name.resolve_column(context, scope),
            // The legacy column adapter discarded the star component. Preserve
            // its binding behavior here; projection expansion is a separate task.
            Self::Star(name) => name.resolve_column(context, scope),
            Self::Operator(operator) => operator.resolve(context, dialect, scope),
            Self::Boolean(boolean) => {
                let arguments = Self::resolve_list(&boolean.arguments, context, dialect, scope)?;
                boolean.operator.resolve(&arguments)
            }
            Self::NullTest(test) => Ok(SemScalarExpr::null_test(
                &test.argument.resolve_in(context, dialect, scope)?,
                test.negated,
            )),
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
            Self::NamedArgument(_)
            | Self::Parameter(_)
            | Self::Default
            | Self::Raise(_)
            | Self::InTable(_) => {
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
        let left = Expr::resolve_optional(&self.left, context, dialect, scope)?;
        let right = Expr::resolve_optional(&self.right, context, dialect, scope)?;
        if dialect == Dialect::sqlite() && !self.kind.without_catalog_operator() {
            // SQLite coercion/affinity is not PostgreSQL operator overload lookup.
            // This was unimplemented by the SQLite analyzer before the migration.
            todo!("SQLite operator semantics");
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
            Self::Between | Self::NotBetween => {
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
            _ => todo!("operator semantic analysis"),
        }
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
        #[cfg(feature = "sqlite")]
        if dialect == Dialect::sqlite() && context.get_type(target.full_name()).is_none() {
            context.report_problem(AnalysisProblem::type_not_found(target.full_name()))?;
        }
        let argument = self.argument.resolve_in(context, dialect, scope)?;
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
            Self::Coalesce => {
                let mut analysis = CoalesceAnalysis::new();
                for argument in &call.arguments {
                    let value = argument.resolve_in(context, dialect, scope)?;
                    analysis.push(context, &value)?;
                }
                return Ok(analysis.finish(context)?.0);
            }
            Self::MinMax => todo!("min/max semantic analysis"),
            Self::Ordinary => {}
        }
        let arguments = Expr::resolve_list(&call.arguments, context, dialect, scope)?;
        FuncCallExpr::resolve_common(
            context,
            &call.name.full_name()?,
            &arguments,
            &call.location.unwrap_or_else(|| StringSpan::new(0, 0)),
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
        analysis.set_argument(&argument);
        for arm in &self.arms {
            let condition = arm.condition.resolve_in(context, dialect, scope)?;
            let result = arm.result.resolve_in(context, dialect, scope)?;
            analysis.push_arm(context, &condition, &result)?;
        }
        let otherwise = Expr::resolve_optional(&self.otherwise, context, dialect, scope)?;
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
        context: &mut Resolution<'_>,
        dialect: Dialect,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let mut value = self.argument.resolve_in(context, dialect, scope)?;
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
