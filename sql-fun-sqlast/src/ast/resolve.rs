mod delete;
mod drop_rule;
mod drop_trigger;
pub(in crate::ast) mod insert;
mod placement;
mod query;
mod returning;
mod rule;
mod trigger;
mod update;

use crate::{
    StringSpan,
    ast::{
        Access, BooleanExpr, BooleanOperator, Call, CallKind, Case, Cast, Dialect, Expr,
        Indirection, Literal, Name, NamedArgument, NullTest, OperatorExpr, OperatorKind, SqlValue,
        Subquery, SubqueryKind, TruthTest, TypeName,
    },
    context::ParseContext,
    sem::{
        AliasName, AnalysisError, AnalysisProblem, ArithExpr, ArrayExpr, ArraySubscript, CaseExpr,
        CaseExprAnalysisContext, CoalesceExpr, ColumnName, DistinctExpr, FullName, FuncCallExpr,
        IndirectionExpr, MinMaxExpr, OperatorName, QueryNameResolution, QueryProblem,
        ReferenceCapture, ResolvedOperator, ScalarAnalysisContext, ScalarConstExpr,
        SelectStatement, SemScalarExpr, SqlValueExpr, TypeReference, problem::RuleProblem,
    },
};

impl Name {
    pub(in crate::ast) fn star_qualifier(&self) -> Result<Option<AliasName>, AnalysisError> {
        match self.0.as_slice() {
            [] => Ok(None),
            [alias] => Ok(Some(AliasName::from(alias.as_str()))),
            _ => AnalysisError::raise_unexpected_input("star qualification"),
        }
    }

    pub(in crate::ast) fn star_relation(&self) -> Result<Option<FullName>, AnalysisError> {
        if self.0.len() < 2 {
            Ok(None)
        } else {
            self.full_name().map(Some)
        }
    }

    pub(in crate::ast) fn full_name(&self) -> Result<FullName, AnalysisError> {
        match self.0.as_slice() {
            [local] => Ok(FullName::from_local_name(local)),
            [schema, local] => Ok(FullName::with_schema(schema, local)),
            [catalog, schema, local] => Ok(FullName::with_catalog(catalog, schema, local)),
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
            [schema, table, column] => {
                return scope.resolve_qualified_column(
                    context,
                    &FullName::with_schema(schema, table),
                    &ColumnName::from(column.as_str()),
                );
            }
            [catalog, schema, table, column] => {
                return scope.resolve_qualified_column(
                    context,
                    &FullName::with_catalog(catalog, schema, table),
                    &ColumnName::from(column.as_str()),
                );
            }
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

impl Name {
    fn append_row_projection(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        fields: &mut Vec<SemScalarExpr>,
    ) -> Result<bool, AnalysisError> {
        if self.is_empty() || context.dialect() != Dialect::postgres() {
            return Ok(false);
        }
        let projection = if let Some(relation) = self.star_relation()? {
            scope.relation_projection_names(context, &relation)?
        } else if let Some(qualifier) = self.star_qualifier()? {
            scope.projection_names(context, &qualifier)?
        } else {
            return Ok(false);
        };
        projection.append_row_fields(context, scope, fields)
    }
}

impl Expr {
    pub(in crate::ast) fn projection_base(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Option<SemScalarExpr>, AnalysisError> {
        match self {
            Self::Indirection(indirection) => indirection.projection_base(context, scope),
            _ => Ok(None),
        }
    }

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
    ) -> Result<SemScalarExpr, AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = self.resolve_in(context, scope);
        let result = context.infer_expression_parameter_types(result);
        context.finish_analysis(result)
    }

    pub(in crate::ast) fn resolve_in(
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
            Self::Star(name) if context.dialect() == Dialect::postgres() && !name.is_empty() => {
                scope.resolve_whole_row(context, &name.full_name()?)
            }
            Self::Star(_) => Self::invalid_star_projection(context),
            Self::Operator(operator) => operator.resolve(context, scope),
            Self::Boolean(boolean) => boolean.resolve(context, scope),
            Self::NullTest(test) => test.resolve(context, scope),
            Self::TruthTest(test) => test.resolve(context, scope),
            Self::Cast(cast) => cast.resolve(context, scope),
            Self::Call(call) => call.resolve(context, scope),
            Self::NamedArgument(argument) => argument.resolve(context, scope),
            Self::Grouping(arguments) => Self::resolve_grouping(arguments, context, scope),
            Self::Case(case) => case.resolve(context, scope),
            Self::Subquery(query) => query.resolve(context, scope),
            Self::Array(items) => Self::resolve_array(items, context, scope, None),
            Self::Row(items) => Self::resolve_row(items, context, scope),
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
            Self::MergeAction => Self::resolve_merge_action(scope),
            Self::Collate(collate) if context.dialect() == Dialect::sqlite() => {
                collate.argument.resolve_in(context, scope)
            }
            // Retain the legacy rejection of raw PostgreSQL syntax that was not
            // accepted by the scalar dispatcher, independently of its conversion.
            Self::Collate(_) | Self::Document(_) | Self::CurrentOf(_) => {
                AnalysisError::raise_unexpected_input("unsupported raw scalar expression")
            }
            Self::Default if context.dialect() == Dialect::postgres() => {
                AnalysisError::raise_unexpected_input("unsupported raw scalar expression")
            }
            // Preserve existing explicit unimplemented markers as well.
            Self::Default | Self::Raise(_) | Self::InTable(_) => {
                todo!("expression semantic analysis")
            }
        }
    }

    fn invalid_star_projection(
        context: &mut impl ParseContext,
    ) -> Result<SemScalarExpr, AnalysisError> {
        context.report_analysis_problem(AnalysisProblem::query(
            QueryProblem::invalid_star_projection_placement(),
        ))?;
        Ok(SemScalarExpr::Unexpected(
            "invalid star projection placement".to_owned(),
        ))
    }

    fn resolve_row(
        items: &[Self],
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let mut fields = Vec::new();
        for item in items {
            if let Self::Star(name) = item
                && name.append_row_projection(context, scope, &mut fields)?
            {
                continue;
            }
            fields.push(item.resolve_in(context, scope)?);
        }
        Ok(SemScalarExpr::row(&fields))
    }

    fn resolve_array(
        items: &[Self],
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        target: Option<&TypeReference>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let mut values = Vec::with_capacity(items.len());
        for item in items {
            values.push(item.resolve_array_target(context, scope, target)?);
        }
        ArrayExpr::resolve(context, &values, target)
    }

    fn resolve_array_target(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        target: Option<&TypeReference>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        crate::sem::with_stack(|| {
            if let Self::Array(items) = self {
                Self::resolve_array(items, context, scope, target)
            } else if let Self::Parenthesized(items) = self
                && let [item] = items.as_slice()
            {
                item.resolve_array_target(context, scope, target)
            } else {
                self.resolve_in(context, scope)
            }
        })
    }

    fn resolve_merge_action(
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        if !scope.merge_action_available() {
            return Err(AnalysisError::unexpected_input_error(
                "merge_action() is only permitted in MERGE RETURNING",
            ));
        }
        Ok(SemScalarExpr::merge_action())
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

    pub(super) fn resolve_optional(
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

impl Expr {
    const fn is_named_argument(&self) -> bool {
        matches!(self, Self::NamedArgument(_))
    }
}

impl NamedArgument {
    fn resolve(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let value = self.argument.resolve_in(context, scope)?;
        Ok(SemScalarExpr::named_argument(
            &self.name,
            &value,
            &self.location,
        ))
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

impl TruthTest {
    fn resolve(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let argument = self.argument.resolve_in(context, scope)?;
        let value =
            crate::sem::TruthTestExpr::resolve(context, &argument, self.value, self.negated)?;
        Ok(SemScalarExpr::truth_test(&value))
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
        let left = Expr::resolve_optional(&self.left, context, scope)?;
        let right = Expr::resolve_optional(&self.right, context, scope)?;
        if let Some(negated) = self.distinctness(context) {
            let (Some(left), Some(right)) = (left, right) else {
                return AnalysisError::raise_unexpected_input("distinctness operands");
            };
            return Ok(SemScalarExpr::distinct(DistinctExpr::resolve(
                context, left, right, negated,
            )?));
        }
        if context.dialect() == Dialect::sqlite() && !self.kind.without_catalog_operator() {
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
    fn distinctness(&self, context: &impl ParseContext) -> Option<bool> {
        if let Some(negated) = self.kind.distinctness() {
            return Some(negated);
        }
        if context.dialect() == Dialect::sqlite() {
            return match self.name.0.as_slice() {
                [name] if name == "is" => Some(true),
                [name] if name == "is not" => Some(false),
                _ => None,
            };
        }
        None
    }
}

impl OperatorKind {
    const fn distinctness(&self) -> Option<bool> {
        match self {
            Self::Distinct => Some(false),
            Self::NotDistinct => Some(true),
            _ => None,
        }
    }

    fn resolve_operator_name(
        &self,
        context: &impl ParseContext,
        name: &Name,
        left: bool,
        right: bool,
    ) -> Result<Option<ResolvedOperator>, AnalysisError> {
        match self {
            Self::Ordinary | Self::Any | Self::All => Ok(Some(ResolvedOperator::resolve(
                context,
                &name.operator_name()?,
                left,
                right,
            ))),
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
            Self::BetweenSymmetric | Self::NotBetweenSymmetric => {
                self.resolve_symmetric_between(context, left, right)
            }
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

    fn resolve_symmetric_between(
        &self,
        context: &mut impl ParseContext,
        left: Option<SemScalarExpr>,
        right: Option<SemScalarExpr>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let left = left.expect("BETWEEN SYMMETRIC argument");
        let values = right
            .expect("BETWEEN SYMMETRIC bounds")
            .into_list_values()?;
        let Ok([start, end]): Result<[SemScalarExpr; 2], _> = values.try_into() else {
            return AnalysisError::raise_unexpected_input("BETWEEN SYMMETRIC bounds");
        };
        ArithExpr::symmetric_between(
            context,
            &left,
            &start,
            &end,
            matches!(self, Self::NotBetweenSymmetric),
        )
    }
}

impl TypeName {
    pub(in crate::ast) fn reference(&self) -> Result<TypeReference, AnalysisError> {
        Ok(TypeReference::concrete_type_ref(
            &self.name.full_name()?,
            !self.array_bounds.is_empty(),
        ))
    }
}

impl Cast {
    fn resolve(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let target = self.target.reference()?;
        let mut resolution = QueryNameResolution::affinity(&target.to_string());
        let target = if context.dialect() == Dialect::postgres() {
            if let Some(canonical) = context.canonicalize_type_ref(&target) {
                resolution = QueryNameResolution::type_reference(&canonical);
                canonical
            } else {
                resolution = QueryNameResolution::not_found();
                context.report_analysis_problem(AnalysisProblem::parameter_constraint(
                    &format!("cast target {target} not found"),
                ))?;
                target
            }
        } else {
            target
        };
        let expected = MinMaxExpr::base_type(context, &target);
        let argument = if context.dialect() == Dialect::postgres()
            && resolution.is_resolved()
            && expected.is_array() == Some(true)
        {
            self.argument
                .resolve_array_target(context, scope, Some(&expected))?
        } else {
            self.argument.resolve_in(context, scope)?
        };
        if context.dialect() == Dialect::postgres()
            && let Some(source) = argument.get_type()
        {
            // Stored declarations may retain unqualified source names.
            let source = if source.is_dynamic() {
                source
            } else {
                context.canonicalize_type_ref(&source).unwrap_or(source)
            };
            // A row constructor can be explicitly assigned a named row identity
            // positionally, without a pg_cast entry for its anonymous type.
            let exact_row_constructor = argument.accepts_exact_row_identity(context, &target);
            if !exact_row_constructor && context.get_explicit_cast(&source, &target).is_none() {
                context.report_analysis_problem(AnalysisProblem::explicit_cast_not_defined(
                    &source, &target,
                ))?;
            }
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
        let aggregate = if self.kind.is_ordinary() && self.over.is_none() {
            let name = context.resolve_function_name_with_search_path(&self.name.full_name()?);
            context.is_aggregate_impl(&name)
        } else {
            false
        };
        let capture = if aggregate {
            Some(ReferenceCapture::default())
        } else {
            None
        };
        let aggregate_scope;
        let argument_scope = if let Some(capture) = &capture {
            aggregate_scope = scope
                .with_grouping_capture(capture)
                .with_grouping_placement("aggregate argument");
            &aggregate_scope
        } else {
            scope
        };
        let mut expression = self.kind.resolve(self, context, argument_scope)?;
        let modifiers = self.resolve_modifiers(context, argument_scope)?;
        if let Some(capture) = &capture {
            let (owner, levels_up, _) = capture.owner(scope.grouping_scope());
            scope.grouping_scope().aggregate(owner, levels_up);
        } else if self.over.is_some() {
            scope.grouping_scope().window();
        }
        expression.set_function_modifiers(modifiers);
        Ok(expression)
    }
}

impl CallKind {
    const fn is_ordinary(&self) -> bool {
        matches!(self, Self::Ordinary)
    }

    fn resolve(
        &self,
        call: &Call,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        if context.dialect() == Dialect::sqlite() {
            if call.name.0.len() == 1 && call.name.0[0].eq_ignore_ascii_case("coalesce") {
                return call.resolve_coalesce(context, scope);
            }
            todo!("SQLite function affinity and overload semantics");
        }
        match self {
            Self::Coalesce => call.resolve_coalesce(context, scope),
            Self::MinMax => {
                let arguments = Expr::resolve_list(&call.arguments, context, scope)?;
                MinMaxExpr::resolve(context, call.name.full_name()?.name().as_str(), &arguments)
            }
            Self::Ordinary => call.resolve_ordinary(context, scope),
        }
    }
}

impl Call {
    fn defers_function_signature(&self) -> bool {
        self.variadic || self.arguments.iter().any(Expr::is_named_argument)
    }

    fn resolve_supplied_call(
        &self,
        context: &mut impl ParseContext,
        arguments: &[SemScalarExpr],
    ) -> Result<FuncCallExpr, AnalysisError> {
        FuncCallExpr::resolve_supplied(
            context,
            &self.name.full_name()?,
            arguments,
            &self.argument_locations,
            &self.location,
        )
    }

    fn resolve_coalesce(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let arguments = Expr::resolve_list(&self.arguments, context, scope)?;
        CoalesceExpr::resolve(context, &arguments)
    }

    fn resolve_ordinary(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let arguments = Expr::resolve_list(&self.arguments, context, scope)?;
        if self.defers_function_signature() {
            return Ok(SemScalarExpr::function_call(
                &self.resolve_supplied_call(context, &arguments)?,
            ));
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
        if let Some(expression) =
            CaseExpr::resolve_postgres_or_untyped(context, &argument, &arms, &otherwise)?
        {
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
        if let Some(identity) = scope.rule_identity()
            && let Some(detail) = self.kind.pending_rule_semantics()
        {
            context.report_analysis_problem(AnalysisProblem::rule(&RuleProblem::definition(
                identity, detail,
            )))?;
        }
        let query = self.query.resolve_in(context, Some(scope))?;
        self.kind.resolve(context, &query)
    }
}

impl SubqueryKind {
    fn resolve(
        &self,
        context: &mut impl ParseContext,
        query: &SelectStatement,
    ) -> Result<SemScalarExpr, AnalysisError> {
        match self {
            Self::Exists => Ok(SemScalarExpr::exists(query)),
            Self::Array => SemScalarExpr::array_subquery(context, query),
            _ => SemScalarExpr::subquery(query),
        }
    }
}

impl SqlValue {
    fn resolve(&self) -> Result<SemScalarExpr, AnalysisError> {
        SqlValueExpr::from_common(&self.name, self.precision)
    }
}

impl Indirection {
    pub(in crate::ast) fn projection_base(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Option<SemScalarExpr>, AnalysisError> {
        let Some((Access::Star, preceding)) = self.items.split_last() else {
            return Ok(None);
        };
        let base = self.argument.resolve_in(context, scope)?;
        Ok(Some(Access::resolve_sequence(
            preceding, context, scope, base,
        )?))
    }

    fn resolve(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let value = self.argument.resolve_in(context, scope)?;
        Access::resolve_sequence(&self.items, context, scope, value)
    }
}

impl Access {
    pub(super) fn resolve_sequence(
        items: &[Self],
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        mut base: SemScalarExpr,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let mut remaining = items;
        while let Some((first, rest)) = remaining.split_first() {
            let count = if context.dialect() == Dialect::postgres() {
                Self::subscript_prefix_length(remaining)
            } else {
                0
            };
            if count > 1 {
                let (group, rest) = remaining.split_at(count);
                let mut subscripts = Vec::with_capacity(count);
                for access in group {
                    subscripts.push(access.resolve_subscript(context, scope)?);
                }
                base = IndirectionExpr::resolved_subscripts(context, &base, &subscripts)?;
                remaining = rest;
            } else {
                base = first.resolve(context, scope, base)?;
                remaining = rest;
            }
        }
        Ok(base)
    }

    fn subscript_prefix_length(items: &[Self]) -> usize {
        let mut count = 0;
        for access in items {
            if !access.is_subscript() {
                break;
            }
            count += 1;
        }
        count
    }

    const fn is_subscript(&self) -> bool {
        matches!(self, Self::Index(_) | Self::Slice(_))
    }

    fn resolve_subscript(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<ArraySubscript, AnalysisError> {
        match self {
            Self::Index(index) => {
                let index = index.resolve_in(context, scope)?;
                Ok(ArraySubscript::index(&index))
            }
            Self::Slice(slice) => {
                let lower = Expr::resolve_optional(&slice.lower, context, scope)?;
                let upper = Expr::resolve_optional(&slice.upper, context, scope)?;
                Ok(ArraySubscript::slice(lower.as_ref(), upper.as_ref()))
            }
            _ => AnalysisError::raise_unexpected_input("array subscript"),
        }
    }

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
            Self::Star => Expr::invalid_star_projection(context),
        }
    }
}

impl Expr {
    fn resolve_grouping(
        arguments: &[Self],
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        context
            .analysis_state_mut()
            .grouping_mut()
            .register_operation();
        let capture = ReferenceCapture::default();
        let argument_scope = scope
            .with_grouping_capture(&capture)
            .with_grouping_placement("GROUPING argument");
        let arguments = Self::resolve_list(arguments, context, &argument_scope)?;
        let (owner, levels_up, placement) = capture.owner(scope.grouping_scope());
        let mut valid = true;
        if capture.invalid_nesting(owner, scope.grouping_scope()) {
            valid = false;
            context.report_analysis_problem(AnalysisProblem::query(
                QueryProblem::invalid_grouping_placement(
                    "GROUPING argument containing an aggregate or window function",
                ),
            ))?;
        }
        scope.grouping_scope().aggregate(owner, levels_up);
        if owner.is_none() || !placement.is_empty() {
            valid = false;
            context.report_analysis_problem(AnalysisProblem::query(
                QueryProblem::invalid_grouping_placement(if placement.is_empty() {
                    "expression outside SELECT"
                } else {
                    &placement
                }),
            ))?;
        }
        if arguments.len() > 31 {
            valid = false;
            context.report_analysis_problem(AnalysisProblem::query(
                QueryProblem::too_many_grouping_arguments(),
            ))?;
        }
        Ok(SemScalarExpr::grouping(&arguments, owner, levels_up, valid))
    }
}
