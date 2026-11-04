mod delete;
mod document;
mod drop_rule;
mod drop_trigger;
pub(crate) mod insert;
mod merge;
mod placement;
mod query;
mod returning;
mod rule;
mod trigger;
mod update;

use crate::{
    CacheContext,
    sem::{MembershipExpr, ProblemLikeError, SamplingInput, TruthTestExpr, WithTypmod, with_stack},
};

use crate::{
    StringSpan,
    ast::{
        Access, BooleanExpr, BooleanOperator, Call, CallKind, Case, Cast, Collate, Dialect, Expr,
        Indirection, Literal, Name, NamedArgument, NullTest, OperatorExpr, OperatorKind, Slice,
        SqlValue, Subquery, SubqueryKind, TruthTest, TypeName,
    },
    context::ParseContext,
    names::{AliasName, ColumnName, FullName, OperatorName, ValidOperatorName},
    sem::{
        AnalysisError, AnalysisProblem, ArithExpr, ArrayExpr, ArraySubscript, CaseExpr,
        CoalesceExpr, CollateExpr, DistinctExpr, FuncCallExpr, IndirectionExpr, MinMaxExpr,
        QueryNameResolution, QueryProblem, ReferenceCapture, ResolvedOperator,
        ScalarAnalysisContext, ScalarConstExpr, SelectStatement, SemScalarExpr, SqlValueExpr,
        SqliteCaseBuilder, TypeReference, problem::RuleProblem,
    },
};

/// Borrows common syntax while semantic conversion owns its interpretation.
pub(crate) struct AstConversion<'a, T: ?Sized> {
    pub(crate) ast: &'a T,
}

pub(crate) const fn convert<T: ?Sized>(ast: &T) -> AstConversion<'_, T> {
    AstConversion { ast }
}

impl<T: ?Sized> std::ops::Deref for AstConversion<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.ast
    }
}

impl AstConversion<'_, Name> {
    pub(crate) fn optional_full_name(&self) -> Result<Option<FullName>, AnalysisError> {
        if self.ast.0.is_empty() {
            Ok(None)
        } else {
            self.full_name().map(Some)
        }
    }

    pub(crate) fn star_qualifier(&self) -> Result<Option<AliasName>, AnalysisError> {
        match self.ast.0.as_slice() {
            [] => Ok(None),
            [alias] => Ok(Some(AliasName::from(alias.as_str()))),
            _ => AnalysisError::raise_unexpected_input("star qualification"),
        }
    }

    pub(crate) fn star_relation(&self) -> Result<Option<FullName>, AnalysisError> {
        if self.ast.0.len() < 2 {
            Ok(None)
        } else {
            self.full_name().map(Some)
        }
    }

    pub(crate) fn full_name(&self) -> Result<FullName, AnalysisError> {
        match self.ast.0.as_slice() {
            [local] => Ok(FullName::from_local_name(local)),
            [schema, local] => Ok(FullName::with_schema(schema, local)),
            [catalog, schema, local] => Ok(FullName::with_catalog(catalog, schema, local)),
            _ => AnalysisError::raise_unexpected_input("qualified name component count"),
        }
    }

    pub(crate) fn operator_name(&self) -> Result<OperatorName, ProblemLikeError> {
        ValidOperatorName::try_from_components(&self.ast.0)
            .map(|name| OperatorName::from_valid(&name))
    }

    pub(crate) fn resolve_column(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let (alias, column) = match self.ast.0.as_slice() {
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
        Ok(if alias.is_none() {
            value.mark_unqualified_column()
        } else {
            value
        })
    }
}

#[cfg(test)]
mod operator_name_validation_tests {
    use crate::sem::convert;

    use crate::ast::Name;

    #[rstest::rstest]
    #[case(&[], "operator_name_syntax")]
    #[case(&[""], "operator_name_syntax")]
    #[case(&["schema", ""], "operator_name_syntax")]
    #[case(&["a", "b", "+"], "cross_database_operator_reference")]
    #[case(&["a", "b", "c", "+"], "cross_database_operator_reference")]
    fn common_names_classify_invalid_components(#[case] parts: &[&str], #[case] code: &str) {
        let parts: Vec<_> = parts.iter().map(|part| (*part).to_owned()).collect();
        let problem = convert(&Name::from_parts(&parts))
            .operator_name()
            .unwrap_err()
            .into_problem();
        assert_eq!(problem.code(), code);
        assert!(problem.render_metadata().is_some());
    }
}

impl AstConversion<'_, Name> {
    pub(crate) fn append_row_projection(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        fields: &mut Vec<SemScalarExpr>,
    ) -> Result<bool, AnalysisError> {
        if self.ast.is_empty() || context.dialect() != Dialect::postgres() {
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

impl AstConversion<'_, Expr> {
    pub(crate) fn projection_base(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Option<SemScalarExpr>, AnalysisError> {
        match self.ast {
            Expr::Indirection(indirection) => {
                convert(indirection.as_ref()).projection_base(context, scope)
            }
            _ => Ok(None),
        }
    }

    pub(crate) fn negated_literal(&self) -> Option<SemScalarExpr> {
        match self.ast {
            Expr::Literal(literal) => convert(literal).negate(),
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
        let result = context.infer_expression_parameter_types(result, &mut CacheContext::new());
        context.finish_analysis(result)
    }

    pub(crate) fn resolve_scalar_in(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let value = self.resolve_in(context, scope)?;
        value.validate_scalar_value(context)?;
        Ok(value)
    }

    pub(crate) fn resolve_in(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        with_stack(|| self.resolve_in_body(context, scope))
    }

    #[inline(never)]
    pub(crate) fn resolve_in_body(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        match self.ast {
            Expr::Parameter(parameter) => Ok(context.parameter(parameter.position)),
            Expr::Literal(value) => Ok(convert(value).resolve()),
            Expr::Column(name) => convert(name).resolve_column(context, scope),
            Expr::Star(name) if context.dialect() == Dialect::postgres() && !name.is_empty() => {
                scope.resolve_whole_row(context, &convert(name).full_name()?)
            }
            Expr::Star(_) => Self::invalid_star_projection(context),
            Expr::Operator(operator) => convert(operator.as_ref()).resolve(context, scope),
            Expr::Boolean(boolean) => convert(boolean.as_ref()).resolve(context, scope),
            Expr::NullTest(test) => convert(test.as_ref()).resolve(context, scope),
            Expr::TruthTest(test) => convert(test.as_ref()).resolve(context, scope),
            Expr::Cast(cast) => convert(cast.as_ref()).resolve(context, scope),
            Expr::Call(call) => convert(call.as_ref()).resolve(context, scope),
            Expr::NamedArgument(argument) => convert(argument.as_ref()).resolve(context, scope),
            Expr::Grouping(arguments) => Self::resolve_grouping(arguments, context, scope),
            Expr::Case(case) => convert(case.as_ref()).resolve(context, scope),
            Expr::Subquery(query) => convert(query.as_ref()).resolve(context, scope),
            Expr::Array(items) => Self::resolve_array(items, context, scope, None),
            Expr::Row(items) => Self::resolve_row(items, context, scope),
            Expr::List(items) => Ok(SemScalarExpr::list(&Self::resolve_list(
                items, context, scope,
            )?)),
            Expr::Parenthesized(items) => Self::resolve_parenthesized(items, context, scope),
            Expr::Indirection(indirection) => convert(indirection.as_ref()).resolve(context, scope),
            Expr::SqlValue(value) => convert(value).resolve(),
            Expr::MergeAction => Self::resolve_merge_action(scope),
            Expr::Collate(collate) => convert(collate.as_ref()).resolve(context, scope),
            // Retain the legacy rejection of raw PostgreSQL syntax that was not
            // accepted by the scalar dispatcher, independently of its conversion.
            Expr::Document(_) | Expr::CurrentOf(_) => {
                AnalysisError::raise_unexpected_input("unsupported raw scalar expression")
            }
            Expr::Default if context.dialect() == Dialect::postgres() => {
                AnalysisError::raise_unexpected_input("unsupported raw scalar expression")
            }
            // Preserve existing explicit unimplemented markers as well.
            Expr::Default | Expr::Raise(_) | Expr::InTable(_) => {
                todo!("expression semantic analysis")
            }
        }
    }

    pub(crate) fn invalid_star_projection(
        context: &mut impl ParseContext,
    ) -> Result<SemScalarExpr, AnalysisError> {
        context.report_analysis_problem(AnalysisProblem::query(
            QueryProblem::invalid_star_projection_placement(),
        ))?;
        Ok(SemScalarExpr::Unexpected(
            "invalid star projection placement".to_owned(),
        ))
    }

    fn resolve_parenthesized(
        items: &[Expr],
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        match items {
            [expression] => convert(expression).resolve_in(context, scope),
            _ => Self::resolve_row(items, context, scope),
        }
    }

    pub(crate) fn resolve_row(
        items: &[Expr],
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let mut fields = Vec::new();
        for item in items {
            if let Expr::Star(name) = item
                && convert(name).append_row_projection(context, scope, &mut fields)?
            {
                continue;
            }
            fields.push(convert(item).resolve_scalar_in(context, scope)?);
        }
        Ok(SemScalarExpr::row(&fields))
    }

    pub(crate) fn resolve_array(
        items: &[Expr],
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        target: Option<&TypeReference>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let mut values = Vec::with_capacity(items.len());
        for item in items {
            values.push(convert(item).resolve_array_target(context, scope, target)?);
        }
        ArrayExpr::resolve(context, &values, target)
    }

    pub(crate) fn resolve_array_target(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        target: Option<&TypeReference>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        with_stack(|| {
            if let Expr::Array(items) = self.ast {
                Self::resolve_array(items, context, scope, target)
            } else if let Expr::Parenthesized(items) = self.ast
                && let [item] = items.as_slice()
            {
                convert(item).resolve_array_target(context, scope, target)
            } else {
                self.resolve_in(context, scope)
            }
        })
    }

    pub(crate) fn resolve_merge_action(
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        if !scope.merge_action_available() {
            return Err(AnalysisError::unexpected_input_error(
                "merge_action() is only permitted in MERGE RETURNING",
            ));
        }
        Ok(SemScalarExpr::merge_action())
    }

    pub(crate) fn resolve_list(
        expressions: &[Expr],
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Vec<SemScalarExpr>, AnalysisError> {
        let mut values = Vec::new();
        for expression in expressions {
            values.push(convert(expression).resolve_in(context, scope)?);
        }
        Ok(values)
    }

    pub(crate) fn resolve_scalar_list(
        expressions: &[Expr],
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Vec<SemScalarExpr>, AnalysisError> {
        let mut values = Vec::new();
        for expression in expressions {
            values.push(convert(expression).resolve_scalar_in(context, scope)?);
        }
        Ok(values)
    }

    pub(crate) fn resolve_optional(
        expression: &Option<Expr>,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Option<SemScalarExpr>, AnalysisError> {
        expression
            .as_ref()
            .map(|expression| convert(expression).resolve_in(context, scope))
            .transpose()
    }
}

impl AstConversion<'_, Expr> {
    pub(crate) const fn is_named_argument(&self) -> bool {
        matches!(self.ast, Expr::NamedArgument(_))
    }
}

impl AstConversion<'_, NamedArgument> {
    pub(crate) fn resolve(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let value = convert(&self.ast.argument).resolve_scalar_in(context, scope)?;
        Ok(SemScalarExpr::named_argument(
            &self.ast.name,
            &value,
            &self.ast.location,
        ))
    }
}

impl AstConversion<'_, Literal> {
    pub(crate) fn resolve(&self) -> SemScalarExpr {
        let value = match self.ast {
            Literal::Null => return ScalarConstExpr::null(),
            Literal::Integer(value) => ScalarConstExpr::new_integer(*value),
            Literal::Numeric(value) => ScalarConstExpr::numeric(value),
            Literal::String(value) => ScalarConstExpr::string(value),
            Literal::Boolean(value) => ScalarConstExpr::boolean(*value),
            Literal::BitString(value) => ScalarConstExpr::bit_string(value),
            Literal::Blob(_) | Literal::Keyword(_) => todo!("literal semantic analysis"),
        };
        SemScalarExpr::literal(&value)
    }

    pub(crate) fn negate(&self) -> Option<SemScalarExpr> {
        match self.ast {
            Literal::Integer(value) => value
                .checked_neg()
                .map(|value| SemScalarExpr::literal(&ScalarConstExpr::new_integer(value))),
            Literal::Numeric(value) => Some(SemScalarExpr::literal(&ScalarConstExpr::numeric(
                &format!("-{value}"),
            ))),
            _ => None,
        }
    }
}

impl AstConversion<'_, BooleanExpr> {
    pub(crate) fn resolve(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let arguments =
            AstConversion::<Expr>::resolve_scalar_list(&self.ast.arguments, context, scope)?;
        convert(&self.ast.operator).resolve(&arguments)
    }
}

impl AstConversion<'_, TruthTest> {
    pub(crate) fn resolve(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let argument = convert(&self.ast.argument).resolve_scalar_in(context, scope)?;
        let value = TruthTestExpr::resolve(context, &argument, self.ast.value, self.ast.negated)?;
        Ok(SemScalarExpr::truth_test(&value))
    }
}

impl AstConversion<'_, NullTest> {
    pub(crate) fn resolve(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let argument = convert(&self.ast.argument).resolve_scalar_in(context, scope)?;
        Ok(SemScalarExpr::null_test(&argument, self.ast.negated))
    }
}

impl AstConversion<'_, BooleanOperator> {
    pub(crate) fn resolve(
        &self,
        arguments: &[SemScalarExpr],
    ) -> Result<SemScalarExpr, AnalysisError> {
        match self.ast {
            BooleanOperator::And => Ok(SemScalarExpr::logical_and(arguments)),
            BooleanOperator::Or => Ok(SemScalarExpr::logical_or(arguments)),
            BooleanOperator::Not => Self::resolve_not(arguments),
        }
    }
}

impl AstConversion<'_, BooleanOperator> {
    fn resolve_not(arguments: &[SemScalarExpr]) -> Result<SemScalarExpr, AnalysisError> {
        match arguments {
            [value] => Ok(SemScalarExpr::logical_not(value)),
            _ => AnalysisError::raise_unexpected_input("NOT argument count"),
        }
    }
}

impl AstConversion<'_, OperatorExpr> {
    pub(crate) fn resolve(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        if context.dialect() == Dialect::sqlite() && self.ast.left.is_none() {
            if self.ast.name.0.as_slice() == ["+"] {
                return convert(self.ast.right.as_ref().expect("unary argument"))
                    .resolve_scalar_in(context, scope);
            }
            if self.ast.name.0.as_slice() == ["-"]
                && let Some(right) = self.ast.right.as_ref()
                && let Some(value) = convert(right).negated_literal()
            {
                return Ok(value);
            }
        }
        let left = AstConversion::<Expr>::resolve_optional(&self.ast.left, context, scope)?;
        let right = AstConversion::<Expr>::resolve_optional(&self.ast.right, context, scope)?;
        if let Some(negated) = convert(&self.ast.kind).membership_negated(&self.ast.name) {
            let left = left.expect("IN left operand");
            let values = right.expect("IN right operand").into_list_values()?;
            return Ok(SemScalarExpr::membership(MembershipExpr::list(
                context, &left, &values, negated,
            )?));
        }
        if let Some(negated) = self.distinctness(context) {
            let (Some(left), Some(right)) = (left, right) else {
                return AnalysisError::raise_unexpected_input("distinctness operands");
            };
            return Ok(SemScalarExpr::distinct(DistinctExpr::resolve(
                context, left, right, negated,
            )?));
        }
        if context.dialect() == Dialect::sqlite() && convert(&self.ast.kind).is_pattern() {
            return self.resolve_sqlite_pattern(context, scope, left, right);
        }
        self.resolve_operator(context, left, right)
    }

    pub(crate) fn resolve_operator(
        &self,
        context: &mut impl ParseContext,
        left: Option<SemScalarExpr>,
        right: Option<SemScalarExpr>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let name = if convert(&self.ast.kind).has_named_operator() {
            match convert(&self.ast.name).operator_name() {
                Ok(name) => name,
                Err(problem) => {
                    problem.report_problem_to(context)?;
                    let name = OperatorName::invalid();
                    let binding = ResolvedOperator::from_definition(context, &name, None);
                    let mut expression = ArithExpr::deferred_operator(&name, left, right);
                    expression.set_operator_binding(binding);
                    return Ok(expression);
                }
            }
        } else {
            OperatorName::invalid()
        };
        if context.dialect() == Dialect::sqlite()
            && !convert(&self.ast.kind).without_catalog_operator()
        {
            if left
                .as_ref()
                .is_some_and(|value| value.get_type().is_none())
                || right
                    .as_ref()
                    .is_some_and(|value| value.get_type().is_none())
            {
                return Ok(ArithExpr::deferred_operator(&name, left, right));
            }
            todo!("SQLite operator semantics");
        }
        let binding = convert(&self.ast.kind).resolve_operator_name(
            context,
            &name,
            left.is_some(),
            right.is_some(),
        );
        let mut expression = convert(&self.ast.kind).resolve(context, &name, left, right)?;
        if let Some(binding) = binding {
            expression.set_operator_binding(binding);
        }
        Ok(expression)
    }

    pub(crate) fn resolve_sqlite_pattern(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        left: Option<SemScalarExpr>,
        right: Option<SemScalarExpr>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        match self.ast.name.0.as_slice() {
            [name] if name == "like" => {
                let (Some(left), Some(right)) = (left, right) else {
                    return AnalysisError::raise_unexpected_input("LIKE operands");
                };
                let escape =
                    AstConversion::<Expr>::resolve_optional(&self.ast.escape, context, scope)?;
                ArithExpr::sqlite_like(
                    context,
                    &left,
                    &right,
                    escape.as_ref(),
                    convert(&self.ast.kind).is_not_like(),
                )
            }
            [name] if name == "glob" => todo!("SQLite GLOB semantics (#907)"),
            [name] if name == "match" => todo!("SQLite MATCH semantics (#908)"),
            [name] if name == "regexp" => todo!("SQLite REGEXP semantics (#909)"),
            _ => AnalysisError::raise_unexpected_input("SQLite pattern operator"),
        }
    }

    pub(crate) fn distinctness(&self, context: &impl ParseContext) -> Option<bool> {
        if let Some(negated) = convert(&self.ast.kind).distinctness() {
            return Some(negated);
        }
        if context.dialect() == Dialect::sqlite() {
            return match self.ast.name.0.as_slice() {
                [name] if name == "is" => Some(true),
                [name] if name == "is not" => Some(false),
                _ => None,
            };
        }
        None
    }
}

#[cfg(all(test, feature = "sqlite"))]
mod test_sqlite_pattern_resolution {
    use crate::{ast::Dialect, test_helpers::membership::context_for};

    use crate::test_helpers::pattern::query;

    #[rstest::rstest]
    #[case::glob("GLOB")]
    #[case::match_operator("MATCH")]
    #[case::regexp("REGEXP")]
    fn sqlite_deferred_patterns_remain_explicitly_unsupported(
        #[case] operator: &str,
        #[values("", "NOT ")] negation: &str,
        #[values("'a'", "$1")] value: &str,
    ) {
        let failure = std::panic::catch_unwind(|| {
            let _ = query(
                &format!("SELECT {value} {negation}{operator} 'a'"),
                context_for(Dialect::sqlite()),
            );
        })
        .expect_err("deferred semantics must not return success");
        let message = if let Some(message) = failure.downcast_ref::<String>() {
            message.as_str()
        } else {
            failure.downcast_ref::<&str>().unwrap()
        };
        assert!(
            message.contains(operator),
            "unexpected unsupported path: {message}"
        );
    }
}

#[cfg(test)]
mod operator_resolution_tests {
    use crate::{
        names::OperatorName,
        sem::{PgBuiltInType, QueryNameResolution},
        test_helpers::{
            operator_names::{addition_context, analyze},
            query::expression,
        },
    };

    #[rstest::rstest]
    #[case("SELECT 1 + 2", None)]
    #[case("SELECT 1 OPERATOR(pg_catalog.+) 2", Some("pg_catalog"))]
    #[case(r#"SELECT 1 OPERATOR("Mixed".+) 2"#, Some("Mixed"))]
    #[case(r#"SELECT 1 OPERATOR("a.b".+) 2"#, Some("a.b"))]
    #[case(r#"SELECT 1 OPERATOR("a""b".+) 2"#, Some("a\"b"))]
    #[case("SELECT $1::int4 OPERATOR(pg_catalog.+) 2", Some("pg_catalog"))]
    fn qualified_operator_binding_survives_inference(
        #[case] sql: &str,
        #[case] schema: Option<&str>,
    ) -> testresult::TestResult {
        let (query, context) = analyze(sql, addition_context(schema))?;
        let value = expression(&query);
        assert_eq!(context.reported_problem_count(), 0);
        assert_eq!(value.get_type(), Some(PgBuiltInType::int4()));
        assert_eq!(
            value.operator_binding().unwrap().resolution(),
            &QueryNameResolution::operator(&OperatorName::from_parts(
                Some(schema.unwrap_or("pg_catalog")),
                "+"
            ))
        );
        Ok(())
    }

    #[rstest::rstest]
    #[case("SELECT 1 OPERATOR(a.b.+) 2")]
    #[case("SELECT 1 OPERATOR(a.b.c.+) 2")]
    #[case("SELECT $1::int4 OPERATOR(a.b.+) 2")]
    fn cross_database_operator_is_recoverable_and_unresolved(
        #[case] sql: &str,
    ) -> testresult::TestResult {
        let (query, context) = analyze(sql, addition_context(None))?;
        assert_eq!(context.reported_problem_count(), 1);
        assert!(context.has_reported_problem("cross_database_operator_reference"));
        let value = expression(&query);
        assert!(value.get_type().is_none());
        assert_eq!(
            value.operator_binding().unwrap().resolution(),
            &QueryNameResolution::not_found()
        );
        // A later independent query still analyzes in the same environment.
        let (query, context) = analyze("SELECT 1 + 2", context)?;
        assert_eq!(expression(&query).get_type(), Some(PgBuiltInType::int4()));
        assert_eq!(context.reported_problem_count(), 1);
        Ok(())
    }

    #[rstest::rstest]
    #[case("SELECT 1 OPERATOR(missing.+) 2")]
    #[case(r#"SELECT 1 OPERATOR("PG_CATALOG".+) 2"#)]
    fn missing_qualified_operator_does_not_bind_unqualified(
        #[case] sql: &str,
    ) -> testresult::TestResult {
        let (query, context) = analyze(sql, addition_context(None))?;
        assert!(context.has_reported_problem("undefined_operator"));
        assert!(!context.has_reported_problem("operator_name_syntax"));
        assert!(!context.has_reported_problem("cross_database_operator_reference"));
        assert_eq!(
            expression(&query).operator_binding().unwrap().resolution(),
            &QueryNameResolution::not_found()
        );
        Ok(())
    }
}

impl AstConversion<'_, OperatorKind> {
    pub(crate) const fn is_pattern(&self) -> bool {
        matches!(
            self.ast,
            OperatorKind::Like
                | OperatorKind::NotLike
                | OperatorKind::ILike
                | OperatorKind::Similar
        )
    }

    pub(crate) const fn is_not_like(&self) -> bool {
        matches!(self.ast, OperatorKind::NotLike)
    }

    pub(crate) fn membership_negated(&self, name: &Name) -> Option<bool> {
        match self.ast {
            OperatorKind::In => Some(name.0.as_slice() == ["<>"]),
            OperatorKind::NotIn => Some(true),
            _ => None,
        }
    }
    pub(crate) const fn distinctness(&self) -> Option<bool> {
        match self.ast {
            OperatorKind::Distinct => Some(false),
            OperatorKind::NotDistinct => Some(true),
            _ => None,
        }
    }

    pub(crate) const fn has_named_operator(&self) -> bool {
        matches!(
            self.ast,
            OperatorKind::Ordinary
                | OperatorKind::Any
                | OperatorKind::All
                | OperatorKind::Like
                | OperatorKind::NotLike
                | OperatorKind::ILike
                | OperatorKind::Similar
        )
    }

    pub(crate) fn resolve_operator_name(
        &self,
        context: &impl ParseContext,
        name: &OperatorName,
        left: bool,
        right: bool,
    ) -> Option<ResolvedOperator> {
        if self.has_named_operator() {
            Some(ResolvedOperator::resolve(context, name, left, right))
        } else {
            None
        }
    }

    pub(crate) const fn without_catalog_operator(&self) -> bool {
        matches!(self.ast, OperatorKind::Between | OperatorKind::NotBetween)
    }
    pub(crate) fn resolve(
        &self,
        context: &mut impl ParseContext,
        name: &OperatorName,
        left: Option<SemScalarExpr>,
        right: Option<SemScalarExpr>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        match self.ast {
            OperatorKind::Ordinary => Ok(ArithExpr::build_op(context, name, left, right)?.0),
            OperatorKind::Like
            | OperatorKind::NotLike
            | OperatorKind::ILike
            | OperatorKind::Similar => ArithExpr::pattern(context, name, left, right),
            OperatorKind::Any => {
                Ok(ArithExpr::build_op_any_resolved(context, name, left, right)?.0)
            }
            OperatorKind::All => {
                Ok(ArithExpr::build_op_all_resolved(context, name, left, right)?.0)
            }
            OperatorKind::NullIf => Ok(ArithExpr::build_null_if(context, left, right)?.0),
            OperatorKind::Between | OperatorKind::NotBetween => {
                self.resolve_between(context, left, right)
            }
            OperatorKind::BetweenSymmetric | OperatorKind::NotBetweenSymmetric => {
                self.resolve_symmetric_between(context, left, right)
            }
            _ => todo!("operator semantic analysis"),
        }
    }
    pub(crate) fn resolve_between(
        &self,
        context: &mut impl ParseContext,
        left: Option<SemScalarExpr>,
        right: Option<SemScalarExpr>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let left = left.expect("BETWEEN argument");
        let values = right.expect("BETWEEN bounds").into_list_values()?;
        let Ok([start, end]): Result<[SemScalarExpr; 2], _> = values.try_into() else {
            return AnalysisError::raise_unexpected_input("BETWEEN bounds");
        };
        if context.dialect() == Dialect::postgres() {
            return ArithExpr::ordinary_between(
                context,
                &left,
                &start,
                &end,
                matches!(self.ast, OperatorKind::NotBetween),
            );
        }
        Ok(if matches!(self.ast, OperatorKind::NotBetween) {
            ArithExpr::not_between(left, (start, end))
        } else {
            ArithExpr::between(left, (start, end))
        })
    }

    pub(crate) fn resolve_symmetric_between(
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
            matches!(self.ast, OperatorKind::NotBetweenSymmetric),
        )
    }
}

#[cfg(test)]
mod test_pattern_resolution {
    use crate::{
        sem::TypeReference as SemTypeReference,
        test_helpers::membership::context as membership_context,
    };

    use crate::{
        context::ParseContext,
        sem::{PgBuiltInType, SemScalarExpr},
        test_helpers::pattern::{context, expression, query},
    };
    use std::collections::BTreeMap;

    #[rstest::rstest]
    #[case::like("LIKE", "~~")]
    #[case::not_like("NOT LIKE", "!~~")]
    #[case::ilike("ILIKE", "~~*")]
    #[case::not_ilike("NOT ILIKE", "!~~*")]
    #[case::similar("SIMILAR TO", "~")]
    #[case::not_similar("NOT SIMILAR TO", "!~")]
    fn postgres_patterns_retain_bound_operators(
        #[case] operator: &str,
        #[case] name: &str,
        #[values("", " ESCAPE '!'")] escape: &str,
    ) -> testresult::TestResult {
        let (query, context) = query(&format!("SELECT 'a' {operator} 'a'{escape}"), context())?;
        assert!(context.problems().is_empty(), "{:?}", context.problems());
        let value = expression(&query);
        assert!(value.operator_binding().unwrap().resolution().is_resolved());
        assert_eq!(
            crate::test_helpers::between::comparison_bindings(value)?,
            vec![name]
        );
        let encoded = ::serde_json::to_value(value)?;
        let decoded: SemScalarExpr = ::serde_json::from_value(encoded.clone())?;
        assert_eq!(::serde_json::to_value(&decoded)?, encoded);
        assert_eq!(decoded.get_type(), Some(PgBuiltInType::bool()));
        if !escape.is_empty() || operator.contains("SIMILAR") {
            let helper = value
                .binary_argument(1)
                .unwrap()
                .as_function_call()
                .unwrap();
            assert!(helper.name_resolution().is_resolved());
            let name = if operator.contains("SIMILAR") {
                "similar_to_escape"
            } else {
                "like_escape"
            };
            assert_eq!(
                helper.name(),
                &crate::names::FullName::with_schema("pg_catalog", name)
            );
        }
        Ok(())
    }

    #[rstest::rstest]
    #[case::like("LIKE")]
    #[case::not_like("NOT LIKE")]
    fn postgres_escape_constraints_preserve_correlated_overloads(
        #[case] operator: &str,
    ) -> testresult::TestResult {
        let (query, context) = query(&format!("SELECT $1 {operator} $2 ESCAPE $3"), context())?;
        assert!(context.problems().is_empty(), "{:?}", context.problems());
        for ty in [
            PgBuiltInType::text(),
            SemTypeReference::from_master_type_name("bytea"),
        ] {
            assert!(context.parameter_constraints().allows(&BTreeMap::from([
                (1, ty.clone()),
                (2, ty.clone()),
                (3, ty),
            ])));
        }
        assert!(!context.parameter_constraints().allows(&BTreeMap::from([
            (1, PgBuiltInType::text()),
            (2, crate::sem::TypeReference::from_master_type_name("bytea")),
            (3, PgBuiltInType::text()),
        ])));
        assert!(
            !context
                .parameter_constraints()
                .allows(&BTreeMap::from([(3, PgBuiltInType::int4())]))
        );
        assert!(
            expression(&query)
                .operator_binding()
                .unwrap()
                .resolution()
                .is_resolved()
        );
        Ok(())
    }

    #[rstest::rstest]
    #[case::ilike("SELECT $1 ILIKE $2 ESCAPE $3")]
    #[case::similar("SELECT $1 SIMILAR TO $2 ESCAPE $3")]
    #[case::repeat("SELECT $1 LIKE $1 ESCAPE $1")]
    #[case::cast("SELECT $1 LIKE $2 ESCAPE $3::text")]
    fn postgres_pattern_constraints_visit_every_operand(
        #[case] sql: &str,
    ) -> testresult::TestResult {
        let (_, context) = query(sql, context())?;
        assert!(context.problems().is_empty(), "{:?}", context.problems());
        let constraints = context.parameter_constraints();
        assert!(constraints.allows(&BTreeMap::from([(1, PgBuiltInType::text())])));
        assert!(!constraints.allows(&BTreeMap::from([(1, PgBuiltInType::int4())])));
        Ok(())
    }

    #[rstest::rstest]
    #[case::pattern_literal("SELECT $1 LIKE 'a%'")]
    #[case::value_literal("SELECT 'a' NOT LIKE $1")]
    #[case::null_literal("SELECT NULL LIKE $1")]
    fn postgres_unknown_pattern_literals_follow_parameter_context(
        #[case] sql: &str,
    ) -> testresult::TestResult {
        let (query, context) = query(sql, context())?;
        assert!(context.problems().is_empty(), "{:?}", context.problems());
        for ty in [
            PgBuiltInType::text(),
            SemTypeReference::from_master_type_name("bytea"),
        ] {
            assert!(
                context
                    .parameter_constraints()
                    .allows(&BTreeMap::from([(1, ty)]))
            );
        }
        assert!(
            !context
                .parameter_constraints()
                .allows(&BTreeMap::from([(1, PgBuiltInType::int4())]))
        );
        assert!(
            expression(&query)
                .operator_binding()
                .unwrap()
                .resolution()
                .is_resolved()
        );
        Ok(())
    }

    #[test]
    fn postgres_pattern_conflicting_use_is_not_accepted() -> testresult::TestResult {
        let (_, context) = query("SELECT $1 LIKE 'a%', NOT $1", context())?;
        assert!(
            context
                .problems()
                .iter()
                .any(|problem| problem.code() == "parameter_constraint")
        );
        assert!(
            !context
                .parameter_constraints()
                .allows(&BTreeMap::from([(1, PgBuiltInType::text())]))
        );
        assert!(
            !context
                .parameter_constraints()
                .allows(&BTreeMap::from([(1, PgBuiltInType::bool())]))
        );
        Ok(())
    }

    #[test]
    fn postgres_missing_pattern_operator_retains_failed_binding() -> testresult::TestResult {
        let (query, context) = query("SELECT 'a' LIKE 'a'", membership_context())?;
        assert!(
            !expression(&query)
                .operator_binding()
                .unwrap()
                .resolution()
                .is_resolved()
        );
        assert_eq!(expression(&query).get_type(), None);
        assert!(
            context
                .problems()
                .iter()
                .any(|problem| problem.code() == "undefined_operator")
        );
        Ok(())
    }
}

impl AstConversion<'_, TypeName> {
    pub(crate) fn reference(&self) -> Result<TypeReference, AnalysisError> {
        Ok(TypeReference::concrete_type_ref(
            &convert(&self.ast.name).full_name()?,
            !self.ast.array_bounds.is_empty(),
        ))
    }
}

impl AstConversion<'_, Cast> {
    pub(crate) fn resolve(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let target = convert(&self.ast.target).reference()?;
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
        let mut argument = if context.dialect() == Dialect::postgres()
            && resolution.is_resolved()
            && expected.is_array() == Some(true)
        {
            convert(&self.ast.argument).resolve_array_target(context, scope, Some(&expected))?
        } else {
            convert(&self.ast.argument).resolve_scalar_in(context, scope)?
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
            // Coerce row fields positionally without a pg_cast entry for the anonymous row.
            let row_cast = argument.coerce_row_cast(context, &target);
            if !row_cast && context.get_explicit_cast(&source, &target).is_none() {
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

impl AstConversion<'_, Call> {
    pub(crate) fn resolve(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let aggregate = if convert(&self.ast.kind).is_ordinary() && self.ast.over.is_none() {
            let name = context
                .resolve_function_name_with_search_path(&convert(&self.ast.name).full_name()?);
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
        let mut expression = convert(&self.ast.kind).resolve(self.ast, context, argument_scope)?;
        let modifiers = self.resolve_modifiers(context, argument_scope)?;
        if let Some(capture) = &capture {
            let (owner, levels_up, placement) = capture.owner(scope.grouping_scope());
            SamplingInput::validate_aggregate_placement(context, &placement)?;
            scope.grouping_scope().aggregate(owner, levels_up);
        } else if self.ast.over.is_some() {
            scope.grouping_scope().window();
        }
        expression.set_function_modifiers(modifiers);
        Ok(expression)
    }
}

impl AstConversion<'_, CallKind> {
    pub(crate) const fn is_ordinary(&self) -> bool {
        matches!(self.ast, CallKind::Ordinary)
    }

    pub(crate) fn resolve(
        &self,
        call: &Call,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        if context.dialect() == Dialect::sqlite() {
            if call.name.0.len() == 1 && call.name.0[0].eq_ignore_ascii_case("coalesce") {
                return convert(call).resolve_coalesce(context, scope);
            }
            todo!("SQLite function affinity and overload semantics");
        }
        match self.ast {
            CallKind::Coalesce => convert(call).resolve_coalesce(context, scope),
            CallKind::MinMax => convert(call).resolve_min_max(context, scope),
            CallKind::Ordinary => convert(call).resolve_ordinary(context, scope),
        }
    }
}

impl AstConversion<'_, Call> {
    pub(crate) fn uses_supplied_binding(&self) -> bool {
        self.ast.variadic
            || self
                .ast
                .arguments
                .iter()
                .any(|value| convert(value).is_named_argument())
    }

    pub(crate) fn resolve_supplied_call(
        &self,
        context: &mut impl ParseContext,
        arguments: &[SemScalarExpr],
    ) -> Result<FuncCallExpr, AnalysisError> {
        FuncCallExpr::resolve_supplied(
            context,
            &convert(&self.ast.name).full_name()?,
            arguments,
            &self.ast.argument_locations,
            &self.ast.location,
            self.ast.variadic,
            self.ast.over.is_some(),
        )
    }

    fn resolve_min_max(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let arguments =
            AstConversion::<Expr>::resolve_scalar_list(&self.ast.arguments, context, scope)?;
        MinMaxExpr::resolve(
            context,
            convert(&self.ast.name).full_name()?.name().as_str(),
            &arguments,
        )
    }

    pub(crate) fn resolve_coalesce(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let arguments =
            AstConversion::<Expr>::resolve_scalar_list(&self.ast.arguments, context, scope)?;
        CoalesceExpr::resolve(context, &arguments)
    }

    pub(crate) fn resolve_ordinary(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let arguments =
            AstConversion::<Expr>::resolve_scalar_list(&self.ast.arguments, context, scope)?;
        if self.uses_supplied_binding() {
            return Ok(SemScalarExpr::function_call(
                &self.resolve_supplied_call(context, &arguments)?,
            ));
        }
        FuncCallExpr::resolve_common(
            context,
            &convert(&self.ast.name).full_name()?,
            &arguments,
            &self.ast.location.unwrap_or_else(|| StringSpan::new(0, 0)),
        )
    }
}

impl AstConversion<'_, Case> {
    pub(crate) fn resolve(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let argument = AstConversion::<Expr>::resolve_optional(&self.ast.argument, context, scope)?;
        if let Some(value) = &argument {
            value.validate_scalar_value(context)?;
        }
        let mut arms = Vec::new();
        for arm in &self.ast.arms {
            let condition = convert(&arm.condition).resolve_scalar_in(context, scope)?;
            let result = convert(&arm.result).resolve_scalar_in(context, scope)?;
            arms.push((condition, result));
        }
        let otherwise =
            AstConversion::<Expr>::resolve_optional(&self.ast.otherwise, context, scope)?;
        if let Some(value) = &otherwise {
            value.validate_scalar_value(context)?;
        }
        if let Some(expression) =
            CaseExpr::resolve_postgres_or_untyped(context, &argument, &arms, &otherwise)?
        {
            return Ok(expression);
        }
        let mut analysis = SqliteCaseBuilder::new();
        analysis.set_argument(&argument);
        for (condition, result) in &arms {
            analysis.push_arm(condition, result);
        }
        analysis.set_default(&otherwise);
        Ok(analysis.finish())
    }
}

impl AstConversion<'_, Subquery> {
    pub(crate) fn resolve(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        if let Some(identity) = scope.rule_identity()
            && let Some(detail) = convert(&self.ast.kind).pending_rule_semantics()
        {
            context.report_analysis_problem(AnalysisProblem::rule(&RuleProblem::definition(
                identity, detail,
            )))?;
        }
        if convert(&self.ast.kind).is_membership(&self.ast.operator) {
            let test = convert(self.ast.test.as_ref().expect("IN test operand"))
                .resolve_in(context, scope)?;
            let query = convert(&self.ast.query).resolve_in(context, Some(scope))?;
            return Ok(SemScalarExpr::membership(MembershipExpr::query(
                context, &test, &query,
            )?));
        }
        let query = convert(&self.ast.query).resolve_in(context, Some(scope))?;
        convert(&self.ast.kind).resolve(context, &query)
    }
}

impl AstConversion<'_, SubqueryKind> {
    pub(crate) fn is_membership(&self, operator: &Name) -> bool {
        matches!(self.ast, SubqueryKind::Any)
            && (operator.0.is_empty() || operator.0.as_slice() == ["="])
    }
    pub(crate) fn resolve(
        &self,
        context: &mut impl ParseContext,
        query: &SelectStatement,
    ) -> Result<SemScalarExpr, AnalysisError> {
        match self.ast {
            SubqueryKind::Exists => Ok(SemScalarExpr::exists(query)),
            SubqueryKind::Array => SemScalarExpr::array_subquery(context, query),
            _ => SemScalarExpr::subquery(query),
        }
    }
}

impl AstConversion<'_, SqlValue> {
    pub(crate) fn resolve(&self) -> Result<SemScalarExpr, AnalysisError> {
        SqlValueExpr::from_common(&self.ast.name, self.ast.precision)
    }
}

impl AstConversion<'_, Indirection> {
    pub(crate) fn projection_base(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Option<SemScalarExpr>, AnalysisError> {
        let Some((Access::Star, preceding)) = self.ast.items.split_last() else {
            return Ok(None);
        };
        let base = convert(&self.ast.argument).resolve_in(context, scope)?;
        Ok(Some(AstConversion::<Access>::resolve_sequence(
            preceding, context, scope, base,
        )?))
    }

    pub(crate) fn resolve(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let value = convert(&self.ast.argument).resolve_in(context, scope)?;
        AstConversion::<Access>::resolve_sequence(&self.ast.items, context, scope, value)
    }
}

impl AstConversion<'_, Access> {
    pub(crate) fn resolve_sequence(
        items: &[Access],
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
                    subscripts.push(convert(access).resolve_subscript(context, scope)?);
                }
                base = IndirectionExpr::resolved_subscripts(context, &base, &subscripts)?;
                remaining = rest;
            } else {
                base = convert(first).resolve(context, scope, base)?;
                remaining = rest;
            }
        }
        Ok(base)
    }

    pub(crate) fn subscript_prefix_length(items: &[Access]) -> usize {
        let mut count = 0;
        for access in items {
            if !convert(access).is_subscript() {
                break;
            }
            count += 1;
        }
        count
    }

    pub(crate) const fn is_subscript(&self) -> bool {
        matches!(self.ast, Access::Index(_) | Access::Slice(_))
    }

    pub(crate) fn resolve_subscript(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<ArraySubscript, AnalysisError> {
        match self.ast {
            Access::Index(index) => ArraySubscript::from_ast_index(index, context, scope),
            Access::Slice(slice) => ArraySubscript::from_ast_slice(slice, context, scope),
            _ => AnalysisError::raise_unexpected_input("array subscript"),
        }
    }

    pub(crate) fn resolve(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        base: SemScalarExpr,
    ) -> Result<SemScalarExpr, AnalysisError> {
        match self.ast {
            Access::Field(name) => IndirectionExpr::resolved_member(context, base, name),
            Access::Index(index) => IndirectionExpr::from_ast_index(index, context, scope, base),
            Access::Slice(slice) => IndirectionExpr::from_ast_slice(slice, context, scope, base),
            Access::Star => AstConversion::<Expr>::invalid_star_projection(context),
        }
    }
}

impl ArraySubscript {
    fn from_ast_index(
        index: &Expr,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        let index = convert(index).resolve_in(context, scope)?;
        Ok(Self::index(&index))
    }

    fn from_ast_slice(
        slice: &Slice,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        let lower = AstConversion::<Expr>::resolve_optional(&slice.lower, context, scope)?;
        let upper = AstConversion::<Expr>::resolve_optional(&slice.upper, context, scope)?;
        Ok(Self::slice(lower.as_ref(), upper.as_ref()))
    }
}

impl IndirectionExpr {
    fn from_ast_index(
        index: &Expr,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        base: SemScalarExpr,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let index = convert(index).resolve_in(context, scope)?;
        Self::resolved_index(context, base, index)
    }

    fn from_ast_slice(
        slice: &Slice,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        base: SemScalarExpr,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let bounds = ArraySubscript::from_ast_slice(slice, context, scope)?;
        Self::resolved_slice(
            context,
            base,
            bounds.lower().cloned(),
            bounds.upper().cloned(),
        )
    }
}

impl AstConversion<'_, Expr> {
    pub(crate) fn resolve_grouping(
        arguments: &[Expr],
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

impl AstConversion<'_, Collate> {
    pub(crate) fn resolve(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let argument = convert(&self.ast.argument).resolve_scalar_in(context, scope)?;
        let value = CollateExpr::resolve(
            context,
            &argument,
            &convert(&self.ast.name).full_name()?,
            &self.ast.location,
        )?;
        Ok(SemScalarExpr::collate(value))
    }
}

impl AstConversion<'_, TypeName> {
    pub(crate) fn declaration_type(
        &self,
        resolved: Option<&TypeReference>,
    ) -> Result<WithTypmod<TypeReference>, AnalysisError> {
        let mut declaration = WithTypmod::new(&self.reference()?, &self.modifier_arguments()?);
        if let Some(resolved) = resolved {
            declaration.interpret_postgres(resolved);
        }
        Ok(declaration)
    }
}

impl AstConversion<'_, TypeName> {
    pub(crate) fn modifier_arguments(&self) -> Result<Vec<String>, AnalysisError> {
        use crate::ast::deparse::{SqlEmit, SqlWriter};
        let mut arguments = Vec::new();
        for modifier in &self.ast.modifiers {
            let mut writer = SqlWriter::new(Dialect::postgres());
            modifier
                .emit(&mut writer)
                .map_err(|error| AnalysisError::unexpected_input_error(&error.to_string()))?;
            arguments.push(writer.into_sql());
        }
        Ok(arguments)
    }
}
