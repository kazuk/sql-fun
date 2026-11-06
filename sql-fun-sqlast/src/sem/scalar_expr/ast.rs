use crate::sem::CommonType;
use crate::{
    CacheContext, StringSpan,
    ast::{
        Access, BooleanExpr, BooleanOperator, Call, CallKind, Case, Cast, Collate, Dialect, Expr,
        Indirection, Literal, Name, NamedArgument, NullTest, OperatorExpr, SqlValue, Subquery,
        SubqueryKind, TruthTest,
    },
    context::ParseContext,
    names::{AliasName, ColumnName, FullName, OperatorName},
    sem::{
        AnalysisError, AnalysisProblem, ArithExpr, ArrayExpr, CaseExpr, CoalesceExpr, CollateExpr,
        ColumnReferenceExpr, DistinctExpr, FuncCallExpr, FunctionModifiers, IndirectionExpr,
        MembershipExpr, MinMaxExpr, PgBuiltInType, QueryNameResolution, QueryOrder, QueryProblem,
        ReferenceCapture, ResolutionFailure, ResolvedOperator, ResultColumnCollection,
        SamplingInput, ScalarAnalysisContext, ScalarConstExpr, SelectStatement, SemScalarExpr,
        SqlValueExpr, SqliteCaseBuilder, TruthTestExpr, TypeReference, UpdateValue,
        problem::RuleProblem, query::ClauseBinding, with_stack,
    },
};

impl SemScalarExpr {
    pub(crate) fn from_ast_column(
        ast: &Name,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        let (alias, column) = match ast.parts() {
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

impl SemScalarExpr {
    pub(crate) fn append_ast_row_projection(
        ast: &Name,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        fields: &mut Vec<Self>,
    ) -> Result<bool, AnalysisError> {
        if ast.is_empty() || context.dialect() != Dialect::postgres() {
            return Ok(false);
        }
        let projection = if let Some(relation) = FullName::from_ast_star_relation(ast)? {
            scope.relation_projection_names(context, &relation)?
        } else if let Some(qualifier) = AliasName::from_ast_star_qualifier(ast)? {
            scope.projection_names(context, &qualifier)?
        } else {
            return Ok(false);
        };
        projection.append_row_fields(context, scope, fields)
    }
}

impl SemScalarExpr {
    pub(crate) fn ast_projection_base(
        ast: &Expr,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Option<Self>, AnalysisError> {
        match ast {
            Expr::Indirection(indirection) => {
                Self::ast_indirection_projection_base(indirection.as_ref(), context, scope)
            }
            _ => Ok(None),
        }
    }
}

impl SemScalarExpr {
    pub(crate) fn negated_ast_literal(ast: &Expr) -> Option<Self> {
        match ast {
            Expr::Literal(literal) => Self::negate_ast_literal(literal),
            _ => None,
        }
    }
}

impl SemScalarExpr {
    pub(crate) fn from_ast(
        ast: &Expr,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = Self::from_ast_in(ast, context, scope);
        let result = context.infer_expression_parameter_types(result, &mut CacheContext::new());
        context.finish_analysis(result)
    }
}

impl SemScalarExpr {
    pub(crate) fn from_ast_scalar_in(
        ast: &Expr,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        let value = Self::from_ast_in(ast, context, scope)?;
        value.validate_scalar_value(context)?;
        Ok(value)
    }
}

impl SemScalarExpr {
    pub(crate) fn from_ast_in(
        ast: &Expr,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        with_stack(|| Self::from_ast_in_body(ast, context, scope))
    }
}

impl SemScalarExpr {
    #[inline(never)]
    pub(crate) fn from_ast_in_body(
        ast: &Expr,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        match ast {
            Expr::Parameter(parameter) => Ok(context.parameter(parameter.position())),
            Expr::Literal(value) => Ok(Self::from_ast_literal(value)),
            Expr::Column(name) => Self::from_ast_column(name, context, scope),
            Expr::Star(name) if context.dialect() == Dialect::postgres() && !name.is_empty() => {
                scope.resolve_whole_row(context, &FullName::from_ast_name(name)?)
            }
            Expr::Star(_) => Self::ast_invalid_star_projection(context),
            Expr::Operator(operator) => {
                Self::from_ast_operator_expr(operator.as_ref(), context, scope)
            }
            Expr::Boolean(boolean) => Self::from_ast_boolean_expr(boolean.as_ref(), context, scope),
            Expr::NullTest(test) => Self::from_ast_null_test(test.as_ref(), context, scope),
            Expr::TruthTest(test) => Self::from_ast_truth_test(test.as_ref(), context, scope),
            Expr::Cast(cast) => Self::from_ast_cast(cast.as_ref(), context, scope),
            Expr::Call(call) => Self::from_ast_call(call.as_ref(), context, scope),
            Expr::NamedArgument(argument) => {
                Self::from_ast_named_argument(argument.as_ref(), context, scope)
            }
            Expr::Grouping(arguments) => Self::from_ast_grouping(arguments, context, scope),
            Expr::Case(case) => Self::from_ast_case(case.as_ref(), context, scope),
            Expr::Subquery(query) => Self::from_ast_subquery(query.as_ref(), context, scope),
            Expr::Array(items) => Self::from_ast_array(items, context, scope, None),
            Expr::Row(items) => Self::from_ast_row(items, context, scope),
            Expr::List(items) => Ok(Self::list(&Self::from_ast_list(items, context, scope)?)),
            Expr::Parenthesized(items) => Self::from_ast_parenthesized(items, context, scope),
            Expr::Indirection(indirection) => {
                Self::from_ast_indirection(indirection.as_ref(), context, scope)
            }
            Expr::SqlValue(value) => Self::from_ast_sql_value(value),
            Expr::MergeAction => Self::from_ast_merge_action(scope),
            Expr::Collate(collate) => Self::from_ast_collate(collate.as_ref(), context, scope),
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
}

impl SemScalarExpr {
    pub(crate) fn ast_invalid_star_projection(
        context: &mut impl ParseContext,
    ) -> Result<Self, AnalysisError> {
        context.report_analysis_problem(AnalysisProblem::query(
            QueryProblem::invalid_star_projection_placement(),
        ))?;
        Ok(Self::Unexpected(
            "invalid star projection placement".to_owned(),
        ))
    }
}

#[cfg(test)]
mod test_row_star_in_scalar_argument_reports_placement {
    use crate::test_helpers::query::table_function_query;

    #[test]
    fn row_star_in_scalar_argument_reports_placement() {
        let (_, context) = table_function_query("SELECT identity((ROW(1)).*)", true);
        assert!(
            context
                .reported_problems()
                .iter()
                .any(|problem| { problem.code() == "query_invalid_star_projection_placement" })
        );
    }
}

impl SemScalarExpr {
    fn from_ast_parenthesized(
        items: &[Expr],
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        match items {
            [expression] => Self::from_ast_in(expression, context, scope),
            _ => Self::from_ast_row(items, context, scope),
        }
    }
}

impl SemScalarExpr {
    pub(crate) fn from_ast_row(
        items: &[Expr],
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        let mut fields = Vec::new();
        for item in items {
            if let Expr::Star(name) = item
                && Self::append_ast_row_projection(name, context, scope, &mut fields)?
            {
                continue;
            }
            fields.push(Self::from_ast_scalar_in(item, context, scope)?);
        }
        Ok(Self::row(&fields))
    }
}

#[cfg(test)]
mod test_every_row_field_retains_its_completed_binding {
    use crate::{ast::Dialect, names::ColumnName, test_helpers::query::resolve};

    #[test]
    fn every_row_field_retains_its_completed_binding() {
        let (query, context) = resolve(
            "SELECT ROW(t.x, public.absent.x, t.x) FROM (SELECT 1 AS x) t",
            Dialect::postgres(),
            false,
        )
        .unwrap();
        assert_eq!(context.reported_problem_count(), 1);
        let row = query
            .result_columns()
            .get_column_at(0)
            .unwrap()
            .expression()
            .unwrap()
            .as_row()
            .unwrap();
        assert_eq!(
            row.fields()[0].constraint_binding(),
            Some(("t", &ColumnName::from("x")))
        );
        assert_eq!(
            row.fields()[1].qualification_failure().unwrap().code(),
            "column_source_not_found"
        );
        assert_eq!(
            row.fields()[2].constraint_binding(),
            Some(("t", &ColumnName::from("x")))
        );
    }
}

impl SemScalarExpr {
    pub(crate) fn from_ast_array(
        items: &[Expr],
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        target: Option<&TypeReference>,
    ) -> Result<Self, AnalysisError> {
        let mut values = Vec::with_capacity(items.len());
        for item in items {
            values.push(Self::from_ast_array_target(item, context, scope, target)?);
        }
        ArrayExpr::resolve(context, &values, target)
    }
}

impl SemScalarExpr {
    pub(crate) fn from_ast_array_target(
        ast: &Expr,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        target: Option<&TypeReference>,
    ) -> Result<Self, AnalysisError> {
        with_stack(|| {
            if let Expr::Array(items) = ast {
                Self::from_ast_array(items, context, scope, target)
            } else if let Expr::Parenthesized(items) = ast
                && let [item] = items.as_slice()
            {
                Self::from_ast_array_target(item, context, scope, target)
            } else {
                Self::from_ast_in(ast, context, scope)
            }
        })
    }
}

impl SemScalarExpr {
    pub(crate) fn from_ast_merge_action(
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        if !scope.merge_action_available() {
            return Err(AnalysisError::unexpected_input_error(
                "merge_action() is only permitted in MERGE RETURNING",
            ));
        }
        Ok(Self::merge_action())
    }
}

impl SemScalarExpr {
    pub(crate) fn from_ast_list(
        expressions: &[Expr],
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Vec<Self>, AnalysisError> {
        let mut values = Vec::new();
        for expression in expressions {
            values.push(Self::from_ast_in(expression, context, scope)?);
        }
        Ok(values)
    }
}

impl SemScalarExpr {
    pub(crate) fn from_ast_scalar_list(
        expressions: &[Expr],
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Vec<Self>, AnalysisError> {
        let mut values = Vec::new();
        for expression in expressions {
            values.push(Self::from_ast_scalar_in(expression, context, scope)?);
        }
        Ok(values)
    }
}

impl SemScalarExpr {
    pub(crate) fn from_ast_optional(
        expression: &Option<Expr>,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Option<Self>, AnalysisError> {
        expression
            .as_ref()
            .map(|expression| Self::from_ast_in(expression, context, scope))
            .transpose()
    }
}

impl SemScalarExpr {
    pub(crate) const fn ast_is_named_argument(ast: &Expr) -> bool {
        matches!(ast, Expr::NamedArgument(_))
    }
}

impl SemScalarExpr {
    pub(crate) fn from_ast_named_argument(
        ast: &NamedArgument,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        let value = Self::from_ast_scalar_in(ast.argument(), context, scope)?;
        Ok(Self::named_argument(ast.name(), &value, ast.location()))
    }
}

impl SemScalarExpr {
    pub(crate) fn from_ast_literal(ast: &Literal) -> Self {
        let value = match ast {
            Literal::Null => return ScalarConstExpr::null(),
            Literal::Integer(value) => ScalarConstExpr::new_integer(*value),
            Literal::Numeric(value) => ScalarConstExpr::numeric(value),
            Literal::String(value) => ScalarConstExpr::string(value),
            Literal::Boolean(value) => ScalarConstExpr::boolean(*value),
            Literal::BitString(value) => ScalarConstExpr::bit_string(value),
            Literal::Blob(_) | Literal::Keyword(_) => todo!("literal semantic analysis"),
        };
        Self::literal(&value)
    }
}

impl SemScalarExpr {
    pub(crate) fn negate_ast_literal(ast: &Literal) -> Option<Self> {
        match ast {
            Literal::Integer(value) => value
                .checked_neg()
                .map(|value| Self::literal(&ScalarConstExpr::new_integer(value))),
            Literal::Numeric(value) => Some(Self::literal(&ScalarConstExpr::numeric(&format!(
                "-{value}"
            )))),
            _ => None,
        }
    }
}

impl SemScalarExpr {
    pub(crate) fn from_ast_boolean_expr(
        ast: &BooleanExpr,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        let arguments = Self::from_ast_scalar_list(ast.arguments(), context, scope)?;
        Self::from_ast_boolean_operator(&ast.operator(), &arguments)
    }
}

impl SemScalarExpr {
    pub(crate) fn from_ast_truth_test(
        ast: &TruthTest,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        let argument = Self::from_ast_scalar_in(ast.argument(), context, scope)?;
        let value = TruthTestExpr::resolve(context, &argument, ast.value(), ast.negated())?;
        Ok(Self::truth_test(&value))
    }
}

impl SemScalarExpr {
    pub(crate) fn from_ast_null_test(
        ast: &NullTest,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        let argument = Self::from_ast_scalar_in(ast.argument(), context, scope)?;
        Ok(Self::null_test(&argument, ast.negated()))
    }
}

impl SemScalarExpr {
    pub(crate) fn from_ast_boolean_operator(
        ast: &BooleanOperator,
        arguments: &[Self],
    ) -> Result<Self, AnalysisError> {
        match ast {
            BooleanOperator::And => Ok(Self::logical_and(arguments)),
            BooleanOperator::Or => Ok(Self::logical_or(arguments)),
            BooleanOperator::Not => Self::from_ast_not(arguments),
        }
    }
}

impl SemScalarExpr {
    fn from_ast_not(arguments: &[Self]) -> Result<Self, AnalysisError> {
        match arguments {
            [value] => Ok(Self::logical_not(value)),
            _ => AnalysisError::raise_unexpected_input("NOT argument count"),
        }
    }
}

impl SemScalarExpr {
    pub(crate) fn from_ast_operator_expr(
        ast: &OperatorExpr,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        if context.dialect() == Dialect::sqlite() && ast.left().is_none() {
            if ast.name().parts() == ["+"] {
                return Self::from_ast_scalar_in(
                    ast.right().as_ref().expect("unary argument"),
                    context,
                    scope,
                );
            }
            if ast.name().parts() == ["-"]
                && let Some(right) = ast.right().as_ref()
                && let Some(value) = Self::negated_ast_literal(right)
            {
                return Ok(value);
            }
        }
        let left = Self::from_ast_optional(ast.left(), context, scope)?;
        let right = Self::from_ast_optional(ast.right(), context, scope)?;
        if let Some(negated) = ArithExpr::ast_membership_negated(&ast.kind(), ast.name()) {
            let left = left.expect("IN left operand");
            let values = right.expect("IN right operand").into_list_values()?;
            return Ok(Self::membership(MembershipExpr::list(
                context, &left, &values, negated,
            )?));
        }
        if let Some(negated) = Self::ast_expression_distinctness(ast, context) {
            let (Some(left), Some(right)) = (left, right) else {
                return AnalysisError::raise_unexpected_input("distinctness operands");
            };
            return Ok(Self::distinct(DistinctExpr::resolve(
                context, left, right, negated,
            )?));
        }
        if context.dialect() == Dialect::sqlite() && ArithExpr::ast_is_pattern(&ast.kind()) {
            return Self::from_ast_sqlite_pattern(ast, context, scope, left, right);
        }
        Self::from_ast_operator_operands(ast, context, left, right)
    }
}

impl SemScalarExpr {
    pub(crate) fn from_ast_operator_operands(
        ast: &OperatorExpr,
        context: &mut impl ParseContext,
        left: Option<Self>,
        right: Option<Self>,
    ) -> Result<Self, AnalysisError> {
        let name = if ArithExpr::ast_has_named_operator(&ast.kind()) {
            match OperatorName::from_ast_name(ast.name()) {
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
            && !ArithExpr::ast_without_catalog_operator(&ast.kind())
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
        let binding = ResolvedOperator::from_ast_operator_name(
            &ast.kind(),
            context,
            &name,
            left.is_some(),
            right.is_some(),
        );
        let mut expression =
            ArithExpr::from_ast_operator_kind(&ast.kind(), context, &name, left, right)?;
        if let Some(binding) = binding {
            expression.set_operator_binding(binding);
        }
        Ok(expression)
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

impl SemScalarExpr {
    pub(crate) fn from_ast_sqlite_pattern(
        ast: &OperatorExpr,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        left: Option<Self>,
        right: Option<Self>,
    ) -> Result<Self, AnalysisError> {
        match ast.name().parts() {
            [name] if name == "like" => {
                let (Some(left), Some(right)) = (left, right) else {
                    return AnalysisError::raise_unexpected_input("LIKE operands");
                };
                let escape = Self::from_ast_optional(ast.escape(), context, scope)?;
                ArithExpr::sqlite_like(
                    context,
                    &left,
                    &right,
                    escape.as_ref(),
                    ArithExpr::ast_is_not_like(&ast.kind()),
                )
            }
            [name] if name == "glob" => todo!("SQLite GLOB semantics (#907)"),
            [name] if name == "match" => todo!("SQLite MATCH semantics (#908)"),
            [name] if name == "regexp" => todo!("SQLite REGEXP semantics (#909)"),
            _ => AnalysisError::raise_unexpected_input("SQLite pattern operator"),
        }
    }
}

#[cfg(all(test, feature = "sqlite"))]
mod test_sqlite_pattern_resolution {
    use crate::{
        ast::Dialect,
        test_helpers::{membership::context_for, pattern::query},
    };

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

impl SemScalarExpr {
    pub(crate) fn ast_expression_distinctness(
        ast: &OperatorExpr,
        context: &impl ParseContext,
    ) -> Option<bool> {
        if let Some(negated) = ArithExpr::ast_operator_distinctness(&ast.kind()) {
            return Some(negated);
        }
        if context.dialect() == Dialect::sqlite() {
            return match ast.name().parts() {
                [name] if name == "is" => Some(true),
                [name] if name == "is not" => Some(false),
                _ => None,
            };
        }
        None
    }
}

impl SemScalarExpr {
    pub(crate) fn from_ast_cast(
        ast: &Cast,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        let target = TypeReference::from_ast(ast.target())?;
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
        let expected = CommonType::base_type(context, &target);
        let mut argument = if context.dialect() == Dialect::postgres()
            && resolution.is_resolved()
            && expected.is_array() == Some(true)
        {
            Self::from_ast_array_target(ast.argument(), context, scope, Some(&expected))?
        } else {
            Self::from_ast_scalar_in(ast.argument(), context, scope)?
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
        let mut expression = Self::explicit_cast(&argument, &target);
        expression.set_cast_resolution(resolution);
        Ok(expression)
    }
}

#[cfg(test)]
mod test_named_row_cast_does_not_accept_mismatched_field_count {
    use crate::test_helpers::query::source_namespace_statement;

    #[test]
    fn named_row_cast_does_not_accept_mismatched_field_count() {
        let analyzed = source_namespace_statement("SELECT (ROW(1)::a.mixed).id");
        assert!(!analyzed.context().reported_problems().is_empty());
    }
}

impl SemScalarExpr {
    pub(crate) fn from_ast_call(
        ast: &Call,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        let aggregate = if Self::ast_is_ordinary(&ast.kind()) && ast.over().is_none() {
            let name = context
                .resolve_function_name_with_search_path(&FullName::from_ast_name(ast.name())?);
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
        let mut expression = Self::from_ast_call_kind(&ast.kind(), ast, context, argument_scope)?;
        let modifiers = FunctionModifiers::from_ast_modifiers(ast, context, argument_scope)?;
        if let Some(capture) = &capture {
            let (owner, levels_up, placement) = capture.owner(scope.grouping_scope());
            SamplingInput::validate_aggregate_placement(context, &placement)?;
            scope.grouping_scope().aggregate(owner, levels_up);
        } else if ast.over().is_some() {
            scope.grouping_scope().window();
        }
        expression.set_function_modifiers(modifiers);
        Ok(expression)
    }
}

impl SemScalarExpr {
    pub(crate) const fn ast_is_ordinary(ast: &CallKind) -> bool {
        matches!(ast, CallKind::Ordinary)
    }
}

impl SemScalarExpr {
    pub(crate) fn from_ast_call_kind(
        ast: &CallKind,
        call: &Call,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        if context.dialect() == Dialect::sqlite() {
            if call.name().parts().len() == 1
                && call.name().parts()[0].eq_ignore_ascii_case("coalesce")
            {
                return Self::from_ast_coalesce_call(call, context, scope);
            }
            todo!("SQLite function affinity and overload semantics");
        }
        match ast {
            CallKind::Coalesce => Self::from_ast_coalesce_call(call, context, scope),
            CallKind::MinMax => Self::from_ast_min_max_call(call, context, scope),
            CallKind::Ordinary => Self::from_ast_ordinary_call(call, context, scope),
        }
    }
}

impl SemScalarExpr {
    pub(crate) fn ast_uses_supplied_binding(ast: &Call) -> bool {
        ast.variadic() || ast.arguments().iter().any(Self::ast_is_named_argument)
    }
}

impl SemScalarExpr {
    fn from_ast_min_max_call(
        ast: &Call,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        let arguments = Self::from_ast_scalar_list(ast.arguments(), context, scope)?;
        MinMaxExpr::resolve(
            context,
            FullName::from_ast_name(ast.name())?.name().as_str(),
            &arguments,
        )
    }
}

impl SemScalarExpr {
    pub(crate) fn from_ast_coalesce_call(
        ast: &Call,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        let arguments = Self::from_ast_scalar_list(ast.arguments(), context, scope)?;
        CoalesceExpr::resolve(context, &arguments)
    }
}

impl SemScalarExpr {
    pub(crate) fn from_ast_ordinary_call(
        ast: &Call,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        let arguments = Self::from_ast_scalar_list(ast.arguments(), context, scope)?;
        if Self::ast_uses_supplied_binding(ast) {
            return Ok(Self::function_call(&FuncCallExpr::from_ast_supplied_call(
                ast, context, &arguments,
            )?));
        }
        FuncCallExpr::resolve_common(
            context,
            &FullName::from_ast_name(ast.name())?,
            &arguments,
            &ast.location().unwrap_or_else(|| StringSpan::new(0, 0)),
        )
    }
}

impl SemScalarExpr {
    pub(crate) fn from_ast_case(
        ast: &Case,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        let argument = Self::from_ast_optional(ast.argument(), context, scope)?;
        if let Some(value) = &argument {
            value.validate_scalar_value(context)?;
        }
        let mut arms = Vec::new();
        for arm in ast.arms() {
            let condition = Self::from_ast_scalar_in(arm.condition(), context, scope)?;
            let result = Self::from_ast_scalar_in(arm.result(), context, scope)?;
            arms.push((condition, result));
        }
        let otherwise = Self::from_ast_optional(ast.otherwise(), context, scope)?;
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

impl SemScalarExpr {
    pub(crate) fn from_ast_subquery(
        ast: &Subquery,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        if let Some(identity) = scope.rule_identity()
            && let Some(detail) = Self::ast_pending_rule_semantics(&ast.kind())
        {
            context.report_analysis_problem(AnalysisProblem::rule(&RuleProblem::definition(
                identity, detail,
            )))?;
        }
        if Self::ast_is_membership(&ast.kind(), ast.operator()) {
            let test = Self::from_ast_in(
                ast.test().as_ref().expect("IN test operand"),
                context,
                scope,
            )?;
            let query = SelectStatement::from_ast_in(ast.query(), context, Some(scope))?;
            return Ok(Self::membership(MembershipExpr::query(
                context, &test, &query,
            )?));
        }
        let query = SelectStatement::from_ast_in(ast.query(), context, Some(scope))?;
        Self::from_ast_subquery_kind(&ast.kind(), context, &query)
    }
}

impl SemScalarExpr {
    pub(crate) fn ast_is_membership(ast: &SubqueryKind, operator: &Name) -> bool {
        matches!(ast, SubqueryKind::Any)
            && (operator.parts().is_empty() || operator.parts() == ["="])
    }
}

impl SemScalarExpr {
    pub(crate) fn from_ast_subquery_kind(
        ast: &SubqueryKind,
        context: &mut impl ParseContext,
        query: &SelectStatement,
    ) -> Result<Self, AnalysisError> {
        match ast {
            SubqueryKind::Exists => Ok(Self::exists(query)),
            SubqueryKind::Array => Self::array_subquery(context, query),
            _ => Self::subquery(query),
        }
    }
}

impl SemScalarExpr {
    pub(crate) fn from_ast_sql_value(ast: &SqlValue) -> Result<Self, AnalysisError> {
        SqlValueExpr::from_common(ast.name(), ast.precision())
    }
}

impl SemScalarExpr {
    pub(crate) fn ast_indirection_projection_base(
        ast: &Indirection,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Option<Self>, AnalysisError> {
        let Some((Access::Star, preceding)) = ast.items().split_last() else {
            return Ok(None);
        };
        let base = Self::from_ast_in(ast.argument(), context, scope)?;
        Ok(Some(IndirectionExpr::from_ast_sequence(
            preceding, context, scope, base,
        )?))
    }
}

impl SemScalarExpr {
    pub(crate) fn from_ast_indirection(
        ast: &Indirection,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        let value = Self::from_ast_in(ast.argument(), context, scope)?;
        IndirectionExpr::from_ast_sequence(ast.items(), context, scope, value)
    }
}

impl SemScalarExpr {
    pub(crate) fn from_ast_grouping(
        arguments: &[Expr],
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        context
            .analysis_state_mut()
            .grouping_mut()
            .register_operation();
        let capture = ReferenceCapture::default();
        let argument_scope = scope
            .with_grouping_capture(&capture)
            .with_grouping_placement("GROUPING argument");
        let arguments = Self::from_ast_list(arguments, context, &argument_scope)?;
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
        Ok(Self::grouping(&arguments, owner, levels_up, valid))
    }
}

#[cfg(test)]
mod test_enforces_argument_limit {
    use crate::{
        ast::Dialect,
        parse_statements,
        sem::SemAst,
        test_helpers::{
            create_cache_context as test_helpers_create_cache_context,
            grouping::groupings,
            query::{grouping_table_context, resolve},
        },
    };

    #[rstest::rstest]
    #[case::maximum(31, true)]
    #[case::too_many(32, false)]
    fn enforces_argument_limit(#[case] count: usize, #[case] valid: bool) {
        let arguments = vec!["a"; count].join(",");
        let (query, context) = resolve(
            &format!("SELECT GROUPING({arguments}) FROM (SELECT 1 AS a) s GROUP BY a"),
            Dialect::postgres(),
            false,
        )
        .unwrap();
        assert_eq!(context.reported_problem_count(), usize::from(!valid));
        let groupings = groupings(&query);
        assert_eq!(groupings[0].arguments().len(), count);
        assert_eq!(groupings[0].is_valid(), valid);
    }

    #[rstest::rstest]
    #[case::insert_value("INSERT INTO t VALUES(GROUPING(1))")]
    #[case::insert_returning("INSERT INTO t VALUES(1) RETURNING GROUPING(a)")]
    #[case::update_value("UPDATE t SET a=GROUPING(a)")]
    #[case::update_returning("UPDATE t SET a=1 RETURNING GROUPING(a)")]
    #[case::delete_where("DELETE FROM t WHERE GROUPING(a) IS NULL")]
    #[case::delete_returning("DELETE FROM t RETURNING GROUPING(a)")]
    fn rejects_grouping_outside_select(#[case] sql: &str) {
        let mut cache_context = test_helpers_create_cache_context();
        let context = grouping_table_context();
        let analysis = SemAst::from_ast(
            &parse_statements(sql, Dialect::postgres())
                .next()
                .unwrap()
                .unwrap(),
            context,
            &mut cache_context,
        )
        .unwrap();
        assert!(
            analysis
                .context()
                .reported_problems()
                .iter()
                .any(|problem| problem.code() == "query_invalid_grouping_placement")
        );
    }
}

impl SemScalarExpr {
    pub(crate) fn from_ast_collate(
        ast: &Collate,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        let argument = Self::from_ast_scalar_in(ast.argument(), context, scope)?;
        let value = CollateExpr::resolve(
            context,
            &argument,
            &FullName::from_ast_name(ast.name())?,
            ast.location(),
        )?;
        Ok(Self::collate(value))
    }
}

impl SemScalarExpr {
    fn from_ast_parenthesized_insert_inference(
        ast: &Expr,
        items: &[Expr],
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, ResolutionFailure> {
        match items {
            [expression] => Self::from_ast_insert_inference(expression, context, scope),
            _ => Self::from_ast_in(ast, context, scope).map_err(Into::into),
        }
    }
}

impl SemScalarExpr {
    pub(crate) fn from_ast_insert_inference(
        ast: &Expr,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, ResolutionFailure> {
        match ast {
            Expr::Collate(collate) => {
                Self::from_ast_insert_inference(collate.argument(), context, scope)
            }
            Expr::Parenthesized(items) => {
                Self::from_ast_parenthesized_insert_inference(ast, items, context, scope)
            }
            _ => Self::from_ast_in(ast, context, scope).map_err(Into::into),
        }
    }
}

impl SemScalarExpr {
    pub(crate) fn from_ast_merge_predicate(
        ast: &Expr,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        let value = Self::from_ast_in(ast, context, scope)?;
        if let Some(ty) = value.get_type()
            && ty != PgBuiltInType::bool()
            && !value
                .as_const()
                .is_some_and(|v| v.is_null() || v.as_string().is_some())
            && context
                .get_implicit_cast(&ty, &PgBuiltInType::bool())
                .is_none()
        {
            return Err(AnalysisError::unexpected_input_error(
                "MERGE condition must have boolean type",
            ));
        }
        Ok(value)
    }
}

impl SemScalarExpr {
    pub(crate) fn from_ast_source_expression(
        ast: &Expr,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Self, AnalysisError> {
        match ast {
            Expr::Call(_) | Expr::SqlValue(_) | Expr::Cast(_) => {
                Self::from_ast_in(ast, context, scope)
            }
            Expr::Operator(operator) if ArithExpr::ast_is_null_if(&operator.kind()) => {
                Self::from_ast_in(ast, context, scope)
            }
            _ => AnalysisError::raise_unexpected_input("ROWS FROM expression"),
        }
    }
}

impl SemScalarExpr {
    pub(crate) fn table_output_name(&self) -> String {
        self.get_column_name()
    }
}

impl SemScalarExpr {
    pub(crate) const fn ast_pending_rule_semantics(ast: &SubqueryKind) -> Option<&'static str> {
        match ast {
            SubqueryKind::Scalar | SubqueryKind::Array => None,
            SubqueryKind::Exists => Some("EXISTS subquery semantics are pending #570"),
            SubqueryKind::Any | SubqueryKind::All | SubqueryKind::Row => {
                Some("quantified/row subquery semantics are pending #571")
            }
            SubqueryKind::MultiExpression => {
                Some("multi-expression subquery semantics are pending #573")
            }
        }
    }
}

impl SemScalarExpr {
    pub(crate) const fn ast_is_rule_boolean(ast: &Expr) -> bool {
        if let Expr::Literal(literal) = ast {
            Self::ast_literal_is_rule_boolean(literal)
        } else {
            false
        }
    }
}

impl SemScalarExpr {
    pub(crate) const fn ast_literal_is_rule_boolean(ast: &Literal) -> bool {
        matches!(ast, Literal::Null | Literal::String(_))
    }
}

impl SemScalarExpr {
    pub(crate) const fn ast_is_untyped_trigger_predicate(ast: &Expr) -> bool {
        match ast {
            Expr::Literal(literal) => Self::ast_literal_is_untyped_trigger_predicate(literal),
            _ => false,
        }
    }
}

impl SemScalarExpr {
    pub(crate) const fn ast_literal_is_untyped_trigger_predicate(ast: &Literal) -> bool {
        matches!(ast, Literal::Null | Literal::String(_))
    }
}

impl SemScalarExpr {
    pub(crate) fn from_ast_subquery_row(
        ast: &Subquery,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        index: usize,
        width: usize,
    ) -> Result<UpdateValue, ResolutionFailure> {
        let query = SelectStatement::from_ast_in(ast.query(), context, Some(scope))?;
        if !query.has_width(width) {
            return Err(
                AnalysisError::unexpected_input_error("row subquery width mismatch").into(),
            );
        }
        Ok(UpdateValue::query(&query, index))
    }
}

impl SemScalarExpr {
    pub(crate) fn from_ast_clause(
        ast: &Expr,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        columns: &ResultColumnCollection,
        binding: ClauseBinding,
    ) -> Result<Self, AnalysisError> {
        if binding.allows_output() {
            if let Some(index) = QueryOrder::ast_ordinal(ast) {
                if let Ok(index) = usize::try_from(index - 1)
                    && let Some(reference) = columns.output_reference(index)
                {
                    return Ok(reference);
                }
                context.report_analysis_problem(AnalysisProblem::query(
                    QueryProblem::position_out_of_range(index),
                ))?;
                return ColumnReferenceExpr::undefined_column_resolved(
                    context,
                    &ColumnName::from(index.to_string().as_str()),
                );
            }
            if let Some(name) = QueryOrder::ast_unqualified_column(ast) {
                let column = ColumnName::from(name);
                if binding.input_first() && scope.has_input_column(context, &column)? {
                    return Self::from_ast_in(ast, context, scope);
                }
                match columns.resolve_output(&column, context) {
                    Ok(Some(index)) => {
                        return Ok(columns
                            .output_reference(index)
                            .expect("resolved output position"));
                    }
                    Err(()) => {
                        return ColumnReferenceExpr::ambiguous_column_resolved(context, &column);
                    }
                    Ok(None) => {}
                }
            }
        }
        if binding.compound() {
            context.report_analysis_problem(AnalysisProblem::query(
                QueryProblem::invalid_compound_order(),
            ))?;
            // Resolve against the compound's empty source scope, never an operand's.
        }
        Self::from_ast_in(ast, context, scope)
    }
}

#[cfg(test)]
mod test_grouping_prefers_the_selected_source_over_an_output_alias {

    use crate::{
        names::FullName,
        test_helpers::{query::source_namespace_statement, query_source_namespace as test_helpers},
    };

    #[test]
    fn grouping_prefers_the_selected_source_over_an_output_alias() {
        let analyzed = source_namespace_statement(
            "SELECT 1 AS z FROM (a.t CROSS JOIN b.t) j(x,y,z,w) GROUP BY z",
        );
        assert!(
            analyzed.context().reported_problems().is_empty(),
            "{:?}",
            analyzed.context().reported_problems()
        );
        let mut query = analyzed.ast().clone().into_select_statemtnt().unwrap();
        let mut bindings = test_helpers::TableBindings::default();
        query.walk(&mut bindings).unwrap();
        bindings.assert_only_relation(&FullName::with_schema("b", "t"));
    }
}

#[cfg(test)]
mod test_binds_aliases_and_ordinals {
    use crate::{
        ast::Dialect,
        sem::QueryGrouping,
        test_helpers::query::{output_index, resolve},
    };

    #[rstest::rstest]
    #[case::order("SELECT n AS x FROM (SELECT 1 AS n) s ORDER BY x", Some(0))]
    #[case::ordinal("SELECT 1 AS x ORDER BY 1", Some(0))]
    #[case::input_precedence("SELECT 1 AS n FROM (SELECT true AS n) s GROUP BY n", None)]
    fn binds_aliases_and_ordinals(#[case] sql: &str, #[case] expected: Option<usize>) {
        let (query, context) = resolve(sql, Dialect::postgres(), false).unwrap();
        assert_eq!(context.reported_problem_count(), 0);
        let query = query.query_clauses().unwrap();
        let expression = if let Some(order) = query.order().first() {
            order.expression()
        } else {
            let QueryGrouping::Expression(expression) =
                &query.body().select_clauses().unwrap().group()[0]
            else {
                panic!("grouping expression");
            };
            expression
        };
        assert_eq!(output_index(expression), expected);
    }
}
