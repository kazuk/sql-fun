use crate::{
    ast::{
        Access, BooleanExpr, BooleanOperator, Call, CallKind, Case, CaseArm, Cast, Collate,
        CurrentOf, Expr, Grouping, InTable, Indirection, Literal, Name, NamedArgument, NullTest,
        OperatorExpr, OperatorKind, Raise, RaiseAction, Slice, SqlValue, Subquery, SubqueryKind,
        TruthTest,
        deparse::{DeparseError, SqlEmit, SqlWriter},
    },
    stack::with_stack,
};

impl SqlEmit for Expr {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        with_stack(|| match self {
            Self::Literal(value) => value.emit(writer),
            Self::Column(value) => value.emit(writer),
            Self::Star(value) => Self::emit_star(value, writer),
            Self::Parameter(value) => value.emit(writer),
            Self::Operator(value) => value.emit(writer),
            Self::Boolean(value) => value.emit(writer),
            Self::NullTest(value) => value.emit(writer),
            Self::TruthTest(value) => value.emit(writer),
            Self::Cast(value) => value.emit(writer),
            Self::Collate(value) => value.emit(writer),
            Self::Call(value) => value.emit(writer),
            Self::Case(value) => value.emit(writer),
            Self::Subquery(value) => value.emit(writer),
            Self::Array(values) => Self::emit_array(values, writer),
            Self::Row(values) => Self::emit_row(values, writer),
            Self::List(values) => {
                writer.sql("(");
                writer.list(values, ", ")?;
                writer.sql(")");
                Ok(())
            }
            Self::Parenthesized(values) => Self::emit_parenthesized(values, writer),
            Self::Indirection(value) => value.emit(writer),
            Self::NamedArgument(value) => value.emit(writer),
            Self::SqlValue(value) => value.emit(writer),
            Self::Default => {
                writer.postgres_only("DEFAULT expression")?;
                writer.sql("DEFAULT");
                Ok(())
            }
            Self::Raise(_) => writer.invalid(
                "RAISE",
                "RAISE requires a trigger body, which this statement does not represent",
            ),
            Self::InTable(value) => value.emit(writer),
            Self::Document(value) => value.emit(writer),
            Self::Grouping(values) => Self::emit_grouping_arguments(values, writer),
            Self::CurrentOf(value) => value.emit(writer),
            Self::MergeAction => {
                writer.postgres_only("MERGE_ACTION")?;
                writer.sql("MERGE_ACTION()");
                Ok(())
            }
        })
    }
}

impl Expr {
    fn emit_star(name: &Name, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        if !writer.is_postgres() && name.0.len() > 1 {
            return writer.invalid(
                "qualified star",
                "SQLite star qualification accepts one table name",
            );
        }
        if name.last_component().is_some() {
            name.emit(writer)?;
            writer.sql(".");
        }
        writer.sql("*");
        Ok(())
    }

    fn emit_array(values: &[Self], writer: &mut SqlWriter) -> Result<(), DeparseError> {
        writer.postgres_only("array expression")?;
        writer.sql("ARRAY[");
        writer.list(values, ", ")?;
        writer.sql("]");
        Ok(())
    }

    fn emit_row(values: &[Self], writer: &mut SqlWriter) -> Result<(), DeparseError> {
        writer.postgres_only("row constructor")?;
        writer.sql("ROW(");
        writer.list(values, ", ")?;
        writer.sql(")");
        Ok(())
    }

    fn emit_parenthesized(values: &[Self], writer: &mut SqlWriter) -> Result<(), DeparseError> {
        if let [value] = values
            && value.emits_parentheses()
        {
            return value.emit(writer);
        }
        if values.is_empty() {
            return writer.invalid("parenthesized expression", "expression list is empty");
        }
        writer.sql("(");
        writer.list(values, ", ")?;
        writer.sql(")");
        Ok(())
    }

    fn emit_grouping_arguments(
        values: &[Self],
        writer: &mut SqlWriter,
    ) -> Result<(), DeparseError> {
        writer.postgres_only("GROUPING")?;
        if values.is_empty() {
            return writer.invalid("GROUPING", "GROUPING requires an argument");
        }
        writer.sql("GROUPING(");
        writer.list(values, ", ")?;
        writer.sql(")");
        Ok(())
    }
}

#[cfg(test)]
mod test_expression_emission {
    use crate::{ast::Dialect, parse_statements};

    #[rstest::rstest]
    #[case::right_associative_subtraction("SELECT a - (b - c)")]
    #[case::multiplication_of_sums("SELECT (a + b) * (c + d)")]
    #[case::boolean_precedence("SELECT NOT (a OR b) AND c")]
    #[case::not_in_list("SELECT a NOT IN (1, 2, 3)")]
    #[case::not_in_subquery("SELECT a NOT IN (SELECT b FROM t)")]
    #[case::empty_in_list_not_postgres("SELECT a IN (1)")]
    #[case::symmetric_bounds("SELECT a NOT BETWEEN SYMMETRIC b AND c")]
    #[case::like_escape("SELECT a NOT LIKE 'x!_%' ESCAPE '!'")]
    #[case::ilike_escape("SELECT a NOT ILIKE 'x!_%' ESCAPE '!'")]
    #[case::similar_escape("SELECT a NOT SIMILAR TO 'x!_%' ESCAPE '!'")]
    #[case::similar_default_escape("SELECT a SIMILAR TO 'x%'")]
    #[case::array_quantifier("SELECT a <> ALL (ARRAY[1, 2])")]
    #[case::subquery_quantifier("SELECT a = ANY (SELECT b FROM t)")]
    #[case::row_subquery("SELECT ROW(a, b) = (SELECT c, d FROM t)")]
    #[case::array_subquery("SELECT ARRAY(SELECT b FROM t)")]
    #[case::case_expression("SELECT CASE a WHEN 1 THEN 'yes' ELSE 'no' END")]
    #[case::nullif("SELECT NULLIF(a, b)")]
    #[case::coalesce("SELECT COALESCE(a, b, c)")]
    #[case::minmax("SELECT GREATEST(a, b), LEAST(a, b)")]
    #[case::extract("SELECT EXTRACT(year FROM CURRENT_DATE)")]
    #[case::position_reversal("SELECT POSITION('x' IN 'abcx')")]
    #[case::substring("SELECT SUBSTRING('abcdef' FROM 2 FOR 3)")]
    #[case::overlay("SELECT OVERLAY('abcdef' PLACING 'xx' FROM 2 FOR 3)")]
    #[case::trim("SELECT TRIM(LEADING 'x' FROM 'xxxabc')")]
    #[case::normalize("SELECT NORMALIZE('abc', NFKC)")]
    #[case::normalized_predicate("SELECT 'abc' IS NOT NFKD NORMALIZED")]
    #[case::time_zone("SELECT CURRENT_TIMESTAMP AT TIME ZONE 'UTC'")]
    #[case::at_local("SELECT CURRENT_TIMESTAMP AT LOCAL")]
    #[case::overlaps("SELECT (a, b) OVERLAPS (c, d)")]
    #[case::collation_for("SELECT COLLATION FOR (a)")]
    #[case::system_user("SELECT SYSTEM_USER")]
    #[case::xml_exists("SELECT XMLEXISTS('/a' PASSING ('<a/>'::xml))")]
    #[case::array_indirection("SELECT (ARRAY[1, 2, 3])[1:2]")]
    #[case::composite_indirection("SELECT (r).field, (r).*")]
    #[case::named_arguments("SELECT f(first => a, second => b)")]
    #[case::aggregate_order("SELECT array_agg(DISTINCT a ORDER BY a) FILTER (WHERE b)")]
    #[case::ordered_set("SELECT percentile_cont(0.5) WITHIN GROUP (ORDER BY a)")]
    fn preserves_postgres_expression_meaning(#[case] sql: &str) {
        let statement = parse_statements(sql, Dialect::postgres())
            .next()
            .unwrap()
            .unwrap();
        let emitted = statement.deparse_sql(Dialect::postgres()).unwrap();
        let original = pg_query::parse(sql, ::pg_query::ParserOptions::default())
            .unwrap()
            .deparse(::pg_query::DeparseOptions::default())
            .unwrap();
        let reparsed = pg_query::parse(&emitted, ::pg_query::ParserOptions::default())
            .unwrap()
            .deparse(::pg_query::DeparseOptions::default())
            .unwrap();
        assert_eq!(original, reparsed, "emitted SQL: {emitted}");
    }

    #[test]
    fn implicit_rows_keep_their_elements_when_emitted_as_row_constructors() {
        let statement = parse_statements("SELECT (a, b)", Dialect::postgres())
            .next()
            .unwrap()
            .unwrap();
        let emitted = statement.deparse_sql(Dialect::postgres()).unwrap();
        let reparsed = pg_query::parse(&emitted, ::pg_query::ParserOptions::default())
            .unwrap()
            .deparse(::pg_query::DeparseOptions::default())
            .unwrap();
        let expected = pg_query::parse("SELECT ROW(a, b)", ::pg_query::ParserOptions::default())
            .unwrap()
            .deparse(::pg_query::DeparseOptions::default())
            .unwrap();
        assert_eq!(expected, reparsed);
    }

    #[cfg(feature = "sqlite")]
    #[rstest::rstest]
    #[case::right_associative_subtraction("SELECT a - (b - c)")]
    #[case::multiplication_of_sums("SELECT (a + b) * (c + d)")]
    #[case::boolean_precedence("SELECT NOT (a OR b) AND c")]
    #[case::between_bounds("SELECT a BETWEEN b AND c")]
    #[case::not_in_subquery("SELECT a NOT IN (SELECT b FROM t)")]
    #[case::scalar_subquery("SELECT (SELECT b FROM t)")]
    #[case::empty_list("SELECT a IN ()")]
    #[case::pattern_escape("SELECT a NOT LIKE 'x!_%' ESCAPE '!'")]
    #[case::glob("SELECT a NOT GLOB 'x*'")]
    #[case::regexp("SELECT a REGEXP '^x'")]
    #[case::match_predicate("SELECT a MATCH 'x'")]
    #[case::in_table("SELECT a IN t")]
    #[case::in_table_function("SELECT a IN f(1, 2)")]
    #[case::cast_and_collate("SELECT CAST(a AS TEXT) COLLATE nocase")]
    #[case::case_expression("SELECT CASE WHEN a THEN b ELSE c END")]
    fn sqlite_expression_rendering_is_stable(#[case] sql: &str) {
        let statement = parse_statements(sql, Dialect::sqlite())
            .next()
            .unwrap()
            .unwrap();
        let emitted = statement.deparse_sql(Dialect::sqlite()).unwrap();
        let reparsed = parse_statements(&emitted, Dialect::sqlite())
            .next()
            .unwrap()
            .unwrap();
        assert_eq!(emitted, reparsed.deparse_sql(Dialect::sqlite()).unwrap());
    }
}

impl Expr {
    const fn emits_parentheses(&self) -> bool {
        match self {
            Self::Operator(_)
            | Self::Boolean(_)
            | Self::NullTest(_)
            | Self::TruthTest(_)
            | Self::Collate(_)
            | Self::InTable(_) => true,
            Self::Subquery(value) => value.kind.emits_parentheses(),
            _ => false,
        }
    }

    fn emit_enclosed(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        if self.emits_parentheses() || matches!(self, Self::Parenthesized(_) | Self::List(_)) {
            self.emit(writer)
        } else {
            writer.sql("(");
            self.emit(writer)?;
            writer.sql(")");
            Ok(())
        }
    }

    fn emit_between_bounds(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        match self {
            Self::List(values) => Self::emit_between_list(values, writer),
            _ => writer.invalid("BETWEEN", "BETWEEN bounds must be an expression list"),
        }
    }

    fn emit_between_list(values: &[Self], writer: &mut SqlWriter) -> Result<(), DeparseError> {
        let [lower, upper] = values else {
            return writer.invalid("BETWEEN", "BETWEEN requires exactly two bounds");
        };
        // Parentheses keep a bound's AND from becoming the BETWEEN separator.
        lower.emit_enclosed(writer)?;
        writer.sql(" AND ");
        upper.emit_enclosed(writer)?;
        Ok(())
    }

    fn emit_in_list(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        match self {
            Self::List(values) => {
                if writer.is_postgres() && values.is_empty() {
                    return writer.invalid("IN", "PostgreSQL IN lists cannot be empty");
                }
                self.emit(writer)
            }
            _ => writer.invalid("IN", "IN requires an expression list"),
        }
    }

    fn emit_pattern(&self, writer: &mut SqlWriter, similar: bool) -> Result<(), DeparseError> {
        match self {
            Self::Call(call) if writer.is_postgres() => call.emit_pattern(writer, similar),
            _ if similar => {
                writer.invalid("SIMILAR TO", "normalized pattern conversion is missing")
            }
            _ => self.emit(writer),
        }
    }

    fn emit_normal_form(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        match self {
            Self::Literal(value) => value.emit_normal_form(writer),
            _ => writer.invalid(
                "normalization form",
                "normalization form must be a string constant",
            ),
        }
    }
}

impl Literal {
    fn emit_normal_form(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        match self {
            Self::String(value) if ["NFC", "NFD", "NFKC", "NFKD"].contains(&value.as_str()) => {
                writer.sql(value);
                Ok(())
            }
            _ => writer.invalid("normalization form", "expected NFC, NFD, NFKC, or NFKD"),
        }
    }
}

impl SqlEmit for OperatorExpr {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        writer.sql("(");
        self.kind.emit_expression(self, writer)?;
        writer.sql(")");
        Ok(())
    }
}

impl OperatorExpr {
    fn operands(&self, writer: &SqlWriter) -> Result<(&Expr, &Expr), DeparseError> {
        let (Some(left), Some(right)) = (&self.left, &self.right) else {
            return writer.invalid("operator", "binary expression requires both operands");
        };
        Ok((left, right))
    }

    fn emit_ordinary(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        if self.escape.is_some() {
            return writer.invalid("operator", "ESCAPE is only legal with a pattern predicate");
        }
        if let Some(left) = &self.left {
            left.emit(writer)?;
            writer.sql(" ");
        }
        self.name.emit_operator(writer)?;
        let Some(right) = &self.right else {
            return writer.invalid(
                "operator",
                "postfix operators are not legal in the supported dialects",
            );
        };
        writer.sql(" ");
        right.emit(writer)
    }

    fn emit_predicate(&self, writer: &mut SqlWriter, keyword: &str) -> Result<(), DeparseError> {
        let (left, right) = self.operands(writer)?;
        left.emit(writer)?;
        writer.sql(keyword);
        right.emit(writer)
    }

    fn emit_nullif(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        let (left, right) = self.operands(writer)?;
        writer.sql("NULLIF(");
        left.emit(writer)?;
        writer.sql(", ");
        right.emit(writer)?;
        writer.sql(")");
        Ok(())
    }

    fn emit_quantified(
        &self,
        writer: &mut SqlWriter,
        quantifier: &str,
    ) -> Result<(), DeparseError> {
        writer.postgres_only("quantified operator")?;
        let (left, right) = self.operands(writer)?;
        left.emit(writer)?;
        writer.sql(" ");
        self.name.emit_operator(writer)?;
        writer.sql(quantifier);
        right.emit(writer)?;
        writer.sql(")");
        Ok(())
    }

    fn emit_in(&self, writer: &mut SqlWriter, negated: bool) -> Result<(), DeparseError> {
        let (left, right) = self.operands(writer)?;
        left.emit(writer)?;
        if negated || self.name.is_local("<>") {
            writer.sql(" NOT IN ");
        } else {
            writer.sql(" IN ");
        }
        right.emit_in_list(writer)
    }

    fn emit_between(
        &self,
        writer: &mut SqlWriter,
        keyword: &str,
        symmetric: bool,
    ) -> Result<(), DeparseError> {
        if symmetric {
            writer.postgres_only("BETWEEN SYMMETRIC")?;
        }
        let (left, right) = self.operands(writer)?;
        left.emit(writer)?;
        writer.sql(keyword);
        right.emit_between_bounds(writer)
    }

    fn emit_like(
        &self,
        writer: &mut SqlWriter,
        insensitive: bool,
        similar: bool,
        negated: bool,
    ) -> Result<(), DeparseError> {
        let (left, right) = self.operands(writer)?;
        left.emit(writer)?;
        let negated = negated
            || self.name.is_local("!~~")
            || self.name.is_local("!~~*")
            || self.name.is_local("!~");
        writer.sql(if negated { " NOT " } else { " " });
        if similar {
            writer.postgres_only("SIMILAR TO")?;
            writer.sql("SIMILAR TO ");
        } else if insensitive {
            writer.postgres_only("ILIKE")?;
            writer.sql("ILIKE ");
        } else if writer.is_postgres() {
            writer.sql("LIKE ");
        } else {
            let keyword = match self.name.last_component() {
                Some("like") => "LIKE ",
                Some("glob") => "GLOB ",
                Some("match") => "MATCH ",
                Some("regexp") => "REGEXP ",
                _ => {
                    return writer.invalid("pattern predicate", "unknown SQLite pattern predicate");
                }
            };
            writer.sql(keyword);
        }
        right.emit_pattern(writer, similar)?;
        if let Some(escape) = &self.escape {
            writer.sql(" ESCAPE ");
            escape.emit(writer)?;
        }
        Ok(())
    }
}

impl OperatorKind {
    fn emit_expression(
        &self,
        value: &OperatorExpr,
        writer: &mut SqlWriter,
    ) -> Result<(), DeparseError> {
        match self {
            Self::Ordinary => value.emit_ordinary(writer),
            Self::Any => value.emit_quantified(writer, " ANY ("),
            Self::All => value.emit_quantified(writer, " ALL ("),
            Self::Distinct => value.emit_predicate(writer, " IS DISTINCT FROM "),
            Self::NotDistinct => value.emit_predicate(writer, " IS NOT DISTINCT FROM "),
            Self::NullIf => value.emit_nullif(writer),
            Self::In => value.emit_in(writer, false),
            Self::NotIn => value.emit_in(writer, true),
            Self::Like => value.emit_like(writer, false, false, false),
            Self::NotLike => value.emit_like(writer, false, false, true),
            Self::ILike => value.emit_like(writer, true, false, false),
            Self::Similar => value.emit_like(writer, false, true, false),
            Self::Between => value.emit_between(writer, " BETWEEN ", false),
            Self::NotBetween => value.emit_between(writer, " NOT BETWEEN ", false),
            Self::BetweenSymmetric => value.emit_between(writer, " BETWEEN SYMMETRIC ", true),
            Self::NotBetweenSymmetric => {
                value.emit_between(writer, " NOT BETWEEN SYMMETRIC ", true)
            }
        }
    }
}

impl SqlEmit for BooleanExpr {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        writer.sql("(");
        self.operator.emit_arguments(&self.arguments, writer)?;
        writer.sql(")");
        Ok(())
    }
}

impl BooleanOperator {
    fn emit_arguments(&self, values: &[Expr], writer: &mut SqlWriter) -> Result<(), DeparseError> {
        match self {
            Self::Not => Self::emit_not(values, writer),
            Self::And => Self::emit_chain(values, " AND ", writer),
            Self::Or => Self::emit_chain(values, " OR ", writer),
        }
    }

    fn emit_not(values: &[Expr], writer: &mut SqlWriter) -> Result<(), DeparseError> {
        let [value] = values else {
            return writer.invalid("NOT", "NOT requires exactly one argument");
        };
        writer.sql("NOT ");
        value.emit(writer)
    }

    fn emit_chain(
        values: &[Expr],
        separator: &str,
        writer: &mut SqlWriter,
    ) -> Result<(), DeparseError> {
        if values.len() < 2 {
            return writer.invalid(
                "Boolean expression",
                "AND and OR require at least two arguments",
            );
        }
        writer.list(values, separator)
    }
}

impl SqlEmit for NullTest {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        writer.sql("(");
        self.argument.emit(writer)?;
        writer.sql(if self.negated {
            " IS NOT NULL)"
        } else {
            " IS NULL)"
        });
        Ok(())
    }
}

impl SqlEmit for TruthTest {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        writer.sql("(");
        self.argument.emit(writer)?;
        writer.sql(if self.negated { " IS NOT " } else { " IS " });
        writer.sql(match self.value {
            Some(true) => "TRUE)",
            Some(false) => "FALSE)",
            None => {
                writer.postgres_only("IS UNKNOWN")?;
                "UNKNOWN)"
            }
        });
        Ok(())
    }
}

impl SqlEmit for Cast {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        writer.sql("CAST(");
        self.argument.emit(writer)?;
        writer.sql(" AS ");
        self.target.emit(writer)?;
        writer.sql(")");
        Ok(())
    }
}

impl SqlEmit for Collate {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        writer.sql("(");
        self.argument.emit(writer)?;
        writer.sql(" COLLATE ");
        self.name.emit(writer)?;
        writer.sql(")");
        Ok(())
    }
}

impl SqlEmit for Call {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        if self.sql_syntax {
            return self.emit_sql_syntax(writer);
        }
        self.kind.emit_name(&self.name, writer)?;
        writer.sql("(");
        self.emit_arguments(writer)?;
        writer.sql(")");
        self.emit_within_group(writer)?;
        if let Some(filter) = &self.filter {
            writer.sql(" FILTER (WHERE ");
            filter.emit(writer)?;
            writer.sql(")");
        }
        if let Some(over) = &self.over {
            writer.sql(" OVER ");
            over.emit(writer)?;
        }
        Ok(())
    }
}

impl CallKind {
    fn emit_name(&self, name: &Name, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        match self {
            Self::Ordinary => name.emit(writer),
            Self::Coalesce => Self::emit_coalesce(name, writer),
            Self::MinMax => Self::emit_min_max(name, writer),
        }
    }

    fn emit_coalesce(name: &Name, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        if !name.is_local("coalesce") {
            return writer.invalid("COALESCE", "special call has an inconsistent name");
        }
        writer.sql("COALESCE");
        Ok(())
    }

    fn emit_min_max(name: &Name, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        writer.postgres_only("GREATEST/LEAST")?;
        if name.is_local("greatest") {
            writer.sql("GREATEST");
        } else if name.is_local("least") {
            writer.sql("LEAST");
        } else {
            return writer.invalid("GREATEST/LEAST", "special call has an inconsistent name");
        }
        Ok(())
    }
}

impl Call {
    fn emit_arguments(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        if self.star {
            if !self.arguments.is_empty()
                || self.distinct
                || self.variadic
                || !self.order.is_empty()
            {
                return writer.invalid(
                    "function call",
                    "star cannot be combined with call arguments or ordering",
                );
            }
            writer.sql("*");
        } else {
            if self.distinct {
                if self.arguments.is_empty() {
                    return writer.invalid("function call", "DISTINCT requires an argument");
                }
                writer.sql("DISTINCT ");
            }
            for (index, argument) in self.arguments.iter().enumerate() {
                if index != 0 {
                    writer.sql(", ");
                }
                if self.variadic && index + 1 == self.arguments.len() {
                    writer.postgres_only("VARIADIC")?;
                    writer.sql("VARIADIC ");
                }
                argument.emit(writer)?;
            }
            if self.variadic && self.arguments.is_empty() {
                return writer.invalid("function call", "VARIADIC requires an argument");
            }
            if !self.order.is_empty() && !self.within_group {
                writer.sql(" ORDER BY ");
                writer.list(&self.order, ", ")?;
            }
        }
        Ok(())
    }

    fn emit_within_group(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        if self.within_group {
            writer.postgres_only("WITHIN GROUP")?;
            writer.sql(" WITHIN GROUP (ORDER BY ");
            let order = if self.within_order.is_empty() {
                &self.order
            } else {
                &self.within_order
            };
            if order.is_empty() {
                return writer.invalid("WITHIN GROUP", "ordered-set call requires ordering");
            }
            writer.list(order, ", ")?;
            writer.sql(")");
        } else if !self.within_order.is_empty() {
            return writer.invalid(
                "function call",
                "WITHIN GROUP ordering has no WITHIN GROUP clause",
            );
        }
        Ok(())
    }

    fn emit_pattern(&self, writer: &mut SqlWriter, similar: bool) -> Result<(), DeparseError> {
        let conversion = if similar {
            "similar_to_escape"
        } else {
            "like_escape"
        };
        if self.name.is_system_function(conversion) {
            let [pattern, tail @ ..] = self.arguments.as_slice() else {
                return writer.invalid(
                    "pattern predicate",
                    "pattern conversion requires an argument",
                );
            };
            if tail.len() > 1 {
                return writer.invalid(
                    "pattern predicate",
                    "pattern conversion has too many arguments",
                );
            }
            pattern.emit(writer)?;
            if let [escape] = tail {
                writer.sql(" ESCAPE ");
                escape.emit(writer)?;
            }
            Ok(())
        } else if similar {
            writer.invalid("SIMILAR TO", "normalized pattern conversion is missing")
        } else {
            self.emit(writer)
        }
    }

    fn expect_arity(
        &self,
        writer: &SqlWriter,
        lower: usize,
        upper: usize,
    ) -> Result<(), DeparseError> {
        if (lower..=upper).contains(&self.arguments.len()) {
            Ok(())
        } else {
            writer.invalid(
                "SQL special function",
                "function has an invalid number of arguments",
            )
        }
    }

    fn emit_sql_syntax(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        writer.postgres_only("SQL special function")?;
        if self.star
            || self.distinct
            || self.variadic
            || self.within_group
            || !self.order.is_empty()
            || !self.within_order.is_empty()
            || self.filter.is_some()
            || self.over.is_some()
        {
            return writer.invalid(
                "SQL special function",
                "special syntax cannot contain aggregate or window modifiers",
            );
        }
        let Some(name) = self.name.last_component() else {
            return writer.invalid("SQL special function", "function name is empty");
        };
        if !self.name.is_system_function(name) {
            return writer.invalid(
                "SQL special function",
                "special syntax must refer to its pg_catalog function",
            );
        }
        match name {
            "system_user" => self.emit_system_user(writer)?,
            "pg_collation_for" => self.emit_collation_for(writer)?,
            "timezone" => self.emit_timezone(writer)?,
            "overlaps" => self.emit_overlaps(writer)?,
            "extract" => self.emit_extract(writer)?,
            "position" => self.emit_position(writer)?,
            "substring" => self.emit_substring(writer)?,
            "overlay" => self.emit_overlay(writer)?,
            "btrim" | "ltrim" | "rtrim" => self.emit_trim(name, writer)?,
            "normalize" | "is_normalized" => self.emit_normalization(name, writer)?,
            "xmlexists" => self.emit_xmlexists(writer)?,
            _ => {
                return writer.invalid(
                    "SQL special function",
                    "unrecognized special SQL function name",
                );
            }
        }
        Ok(())
    }
}

impl Call {
    fn emit_system_user(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        self.expect_arity(writer, 0, 0)?;
        writer.sql("SYSTEM_USER");
        Ok(())
    }

    fn emit_collation_for(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        self.expect_arity(writer, 1, 1)?;
        writer.sql("COLLATION FOR (");
        self.arguments[0].emit(writer)?;
        writer.sql(")");
        Ok(())
    }

    fn emit_timezone(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        self.expect_arity(writer, 1, 2)?;
        writer.sql("(");
        if let [zone, argument] = self.arguments.as_slice() {
            argument.emit(writer)?;
            writer.sql(" AT TIME ZONE ");
            zone.emit(writer)?;
        } else {
            self.arguments[0].emit(writer)?;
            writer.sql(" AT LOCAL");
        }
        writer.sql(")");
        Ok(())
    }

    fn emit_overlaps(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        self.expect_arity(writer, 4, 4)?;
        writer.sql("((");
        writer.list(&self.arguments[..2], ", ")?;
        writer.sql(") OVERLAPS (");
        writer.list(&self.arguments[2..], ", ")?;
        writer.sql("))");
        Ok(())
    }

    fn emit_extract(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        self.expect_arity(writer, 2, 2)?;
        writer.sql("EXTRACT(");
        self.arguments[0].emit(writer)?;
        writer.sql(" FROM ");
        self.arguments[1].emit(writer)?;
        writer.sql(")");
        Ok(())
    }

    fn emit_position(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        self.expect_arity(writer, 2, 2)?;
        writer.sql("POSITION(");
        self.arguments[1].emit(writer)?;
        writer.sql(" IN ");
        self.arguments[0].emit(writer)?;
        writer.sql(")");
        Ok(())
    }

    fn emit_substring(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        self.expect_arity(writer, 2, 3)?;
        writer.sql("SUBSTRING(");
        self.arguments[0].emit(writer)?;
        writer.sql(" FROM ");
        self.arguments[1].emit(writer)?;
        if self.arguments.len() == 3 {
            writer.sql(" FOR ");
            self.arguments[2].emit(writer)?;
        }
        writer.sql(")");
        Ok(())
    }

    fn emit_overlay(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        self.expect_arity(writer, 3, 4)?;
        writer.sql("OVERLAY(");
        self.arguments[0].emit(writer)?;
        writer.sql(" PLACING ");
        self.arguments[1].emit(writer)?;
        writer.sql(" FROM ");
        self.arguments[2].emit(writer)?;
        if self.arguments.len() == 4 {
            writer.sql(" FOR ");
            self.arguments[3].emit(writer)?;
        }
        writer.sql(")");
        Ok(())
    }

    fn emit_xmlexists(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        self.expect_arity(writer, 2, 2)?;
        writer.sql("XMLEXISTS(");
        self.arguments[0].emit(writer)?;
        writer.sql(" PASSING ");
        self.arguments[1].emit(writer)?;
        writer.sql(")");
        Ok(())
    }

    fn emit_trim(&self, name: &str, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        self.expect_arity(writer, 1, usize::MAX)?;
        writer.sql(match name {
            "ltrim" => "TRIM(LEADING FROM ",
            "rtrim" => "TRIM(TRAILING FROM ",
            _ => "TRIM(BOTH FROM ",
        });
        writer.list(&self.arguments, ", ")?;
        writer.sql(")");
        Ok(())
    }

    fn emit_normalization(&self, name: &str, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        self.expect_arity(writer, 1, 2)?;
        if name == "normalize" {
            writer.sql("NORMALIZE(");
            self.arguments[0].emit(writer)?;
            if self.arguments.len() == 2 {
                writer.sql(", ");
                self.arguments[1].emit_normal_form(writer)?;
            }
            writer.sql(")");
        } else {
            writer.sql("(");
            self.arguments[0].emit(writer)?;
            writer.sql(" IS ");
            if self.arguments.len() == 2 {
                self.arguments[1].emit_normal_form(writer)?;
                writer.sql(" ");
            }
            writer.sql("NORMALIZED)");
        }
        Ok(())
    }
}

impl SqlEmit for Case {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        if self.arms.is_empty() {
            return writer.invalid("CASE", "CASE requires a WHEN arm");
        }
        writer.sql("CASE");
        if let Some(argument) = &self.argument {
            writer.sql(" ");
            argument.emit(writer)?;
        }
        writer.list(&self.arms, "")?;
        if let Some(otherwise) = &self.otherwise {
            writer.sql(" ELSE ");
            otherwise.emit(writer)?;
        }
        writer.sql(" END");
        Ok(())
    }
}

impl SqlEmit for CaseArm {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        writer.sql(" WHEN ");
        self.condition.emit(writer)?;
        writer.sql(" THEN ");
        self.result.emit(writer)
    }
}

impl SqlEmit for Subquery {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        self.kind.emit_subquery(self, writer)
    }
}

impl SubqueryKind {
    const fn emits_parentheses(&self) -> bool {
        matches!(
            self,
            Self::Scalar | Self::MultiExpression | Self::Any | Self::All | Self::Row
        )
    }

    fn emit_subquery(&self, value: &Subquery, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        match self {
            Self::Scalar | Self::MultiExpression => value.emit_enclosed_query("(", writer),
            Self::Exists => value.emit_enclosed_query("EXISTS (", writer),
            Self::Array => value.emit_array_query(writer),
            Self::Any | Self::All | Self::Row => value.emit_comparison(writer),
        }
    }

    fn uses_in(&self, value: &Subquery, writer: &SqlWriter) -> bool {
        matches!(self, Self::Any)
            && (value.operator.last_component().is_none()
                || (!writer.is_postgres() && value.operator.is_local("=")))
    }

    const fn comparison_suffix(&self) -> &'static str {
        match self {
            Self::Any => " ANY (",
            Self::All => " ALL (",
            _ => " (",
        }
    }
}

impl Subquery {
    fn emit_enclosed_query(
        &self,
        prefix: &str,
        writer: &mut SqlWriter,
    ) -> Result<(), DeparseError> {
        writer.sql(prefix);
        self.query.emit(writer)?;
        writer.sql(")");
        Ok(())
    }

    fn emit_array_query(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        writer.postgres_only("array subquery")?;
        self.emit_enclosed_query("ARRAY(", writer)
    }

    fn emit_comparison(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        let Some(test) = &self.test else {
            return writer.invalid(
                "subquery predicate",
                "subquery predicate requires a left operand",
            );
        };
        writer.sql("(");
        test.emit(writer)?;
        if self.kind.uses_in(self, writer) {
            writer.sql(" IN (");
        } else {
            writer.postgres_only("quantified subquery")?;
            writer.sql(" ");
            self.operator.emit_operator(writer)?;
            writer.sql(self.kind.comparison_suffix());
        }
        self.query.emit(writer)?;
        writer.sql("))");
        Ok(())
    }
}

impl SqlEmit for Indirection {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        writer.postgres_only("indirection")?;
        if self.items.is_empty() {
            return writer.invalid("indirection", "indirection requires a field or subscript");
        }
        writer.sql("(");
        self.argument.emit(writer)?;
        writer.sql(")");
        writer.list(&self.items, "")
    }
}

impl SqlEmit for Access {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        writer.postgres_only("field or subscript access")?;
        match self {
            Self::Field(name) => {
                writer.sql(".");
                writer.ident(name)
            }
            Self::Star => {
                writer.sql(".*");
                Ok(())
            }
            Self::Index(value) => {
                writer.sql("[");
                value.emit(writer)?;
                writer.sql("]");
                Ok(())
            }
            Self::Slice(value) => value.emit(writer),
        }
    }
}

impl SqlEmit for Slice {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        writer.postgres_only("array slice")?;
        writer.sql("[");
        if let Some(lower) = &self.lower {
            lower.emit(writer)?;
        }
        writer.sql(":");
        if let Some(upper) = &self.upper {
            upper.emit(writer)?;
        }
        writer.sql("]");
        Ok(())
    }
}

impl SqlEmit for NamedArgument {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        writer.postgres_only("named argument")?;
        writer.ident(&self.name)?;
        writer.sql(" => ");
        self.argument.emit(writer)
    }
}

impl SqlEmit for SqlValue {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        let precision_allowed = match self.name.as_str() {
            "current_time" | "current_timestamp" => true,
            "localtime" | "localtimestamp" => {
                writer.postgres_only("local time value")?;
                true
            }
            "current_date" => false,
            "current_role" | "current_user" | "user" | "session_user" | "current_catalog"
            | "current_schema" => {
                writer.postgres_only("SQL session value")?;
                false
            }
            _ => return writer.invalid("SQL value", "unknown SQL value function"),
        };
        writer.sql(&self.name.to_ascii_uppercase());
        if let Some(precision) = self.precision {
            writer.postgres_only("SQL time precision")?;
            if !precision_allowed || precision < 0 {
                return writer.invalid("SQL value", "precision is invalid for this SQL value");
            }
            writer.sql("(");
            writer.number(precision);
            writer.sql(")");
        }
        Ok(())
    }
}

impl SqlEmit for Raise {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        if writer.is_postgres() {
            return writer.invalid(
                "RAISE",
                "RAISE expressions are only legal in SQLite triggers",
            );
        }
        writer.sql("RAISE(");
        self.action.emit_raise(&self.message, writer)?;
        writer.sql(")");
        Ok(())
    }
}

impl RaiseAction {
    fn emit_raise(
        &self,
        message: &Option<Expr>,
        writer: &mut SqlWriter,
    ) -> Result<(), DeparseError> {
        match self {
            Self::Ignore => Self::emit_ignore(message, writer),
            Self::Rollback => Self::emit_message("ROLLBACK", message, writer),
            Self::Abort => Self::emit_message("ABORT", message, writer),
            Self::Fail => Self::emit_message("FAIL", message, writer),
            Self::Replace => writer.invalid("RAISE", "REPLACE is not a legal RAISE action"),
        }
    }

    fn emit_ignore(message: &Option<Expr>, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        if message.is_some() {
            return writer.invalid("RAISE(IGNORE)", "IGNORE cannot have an error message");
        }
        writer.sql("IGNORE");
        Ok(())
    }

    fn emit_message(
        keyword: &str,
        message: &Option<Expr>,
        writer: &mut SqlWriter,
    ) -> Result<(), DeparseError> {
        writer.sql(keyword);
        let Some(message) = message else {
            return writer.invalid("RAISE", "RAISE action requires an error message");
        };
        writer.sql(", ");
        message.emit(writer)
    }
}

impl SqlEmit for InTable {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        if writer.is_postgres() {
            return writer.invalid("IN table", "IN table syntax is only legal in SQLite");
        }
        writer.sql("(");
        self.argument.emit(writer)?;
        writer.sql(if self.negated { " NOT IN " } else { " IN " });
        self.table.emit(writer)?;
        if !self.arguments.is_empty() {
            writer.sql("(");
            writer.list(&self.arguments, ", ")?;
            writer.sql(")");
        }
        writer.sql(")");
        Ok(())
    }
}

impl SqlEmit for Grouping {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        match self {
            Self::Expression(value) => value.emit(writer),
            Self::Empty => Self::emit_empty(writer),
            Self::Rollup(values) => Self::emit_group(values, "ROLLUP(", writer),
            Self::Cube(values) => Self::emit_group(values, "CUBE(", writer),
            Self::Sets(values) => Self::emit_group(values, "GROUPING SETS (", writer),
            Self::Simple(values) => Self::emit_group(values, "(", writer),
        }
    }
}

impl Grouping {
    fn emit_empty(writer: &mut SqlWriter) -> Result<(), DeparseError> {
        writer.postgres_only("empty grouping set")?;
        writer.sql("()");
        Ok(())
    }

    fn emit_group(
        values: &[Self],
        prefix: &str,
        writer: &mut SqlWriter,
    ) -> Result<(), DeparseError> {
        writer.postgres_only("grouping set")?;
        writer.sql(prefix);
        writer.list(values, ", ")?;
        writer.sql(")");
        Ok(())
    }
}

impl SqlEmit for CurrentOf {
    fn emit(&self, writer: &mut SqlWriter) -> Result<(), DeparseError> {
        writer.postgres_only("CURRENT OF")?;
        writer.sql("CURRENT OF ");
        if !self.name.is_empty() && self.parameter == 0 {
            writer.ident(&self.name)
        } else if self.name.is_empty() && self.parameter > 0 {
            writer.sql("$");
            writer.number(self.parameter);
            Ok(())
        } else {
            writer.invalid(
                "CURRENT OF",
                "CURRENT OF requires exactly one cursor name or positive parameter",
            )
        }
    }
}
