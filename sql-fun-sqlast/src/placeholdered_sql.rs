use ::sql_fun_core::{PlaceholderedSql, PlaceholderedSqlToken};

use crate::ast::Dialect;

/// Errors returned when rendering sql-fun placeholders as dialect SQL.
#[derive(Debug, thiserror::Error)]
pub enum PlaceholderedSqlRenderError {
    /// A query parameter is absent from the caller-supplied ordering.
    #[error("query parameter {0} is absent from the supplied parameter ordering")]
    ParameterNotSupplied(String),
    /// A fragment placeholder remains after fragment expansion.
    #[error("unresolved SQL fragment placeholder {0}")]
    UnresolvedFragment(String),
    /// SQLite rendering requires the crate's `sqlite` feature.
    #[error("SQLite placeholder rendering requires the `sqlite` feature")]
    SqliteFeatureRequired,
}

impl PlaceholderedSqlRenderError {
    fn parameter_not_supplied<T>(name: &str) -> Result<T, Self> {
        Err(Self::ParameterNotSupplied(String::from(name)))
    }

    fn unresolved_fragment<T>(name: &str) -> Result<T, Self> {
        Err(Self::UnresolvedFragment(String::from(name)))
    }

    #[cfg(not(feature = "sqlite"))]
    const fn sqlite_feature_required<T>() -> Result<T, Self> {
        Err(Self::SqliteFeatureRequired)
    }
}

impl Dialect {
    /// Render a fully expanded sql-fun query as SQL for this dialect.
    ///
    /// `parameter_order` is the source of truth for numbered placeholders: a
    /// parameter's number is its zero-based position in that slice plus one.
    /// Names unused by `sql` retain their positions, so the output can have
    /// non-contiguous placeholder numbers.
    ///
    /// # Errors
    ///
    /// Returns [`PlaceholderedSqlRenderError`] when the SQL contains an
    /// unresolved fragment, a parameter not present in `parameter_order`, or
    /// SQLite was selected without the `sqlite` feature.
    pub fn render_placeholdered_sql<T>(
        self,
        sql: &PlaceholderedSql,
        parameter_order: &[T],
    ) -> Result<String, PlaceholderedSqlRenderError>
    where
        T: AsRef<str>,
    {
        let mut result = String::new();
        for token in sql.tokens() {
            match token {
                PlaceholderedSqlToken::ParameterPlaceholder(parameter) => {
                    let Some(position) = parameter_order
                        .iter()
                        .position(|name| name.as_ref() == parameter.name().as_str())
                    else {
                        return PlaceholderedSqlRenderError::parameter_not_supplied(
                            parameter.name().as_str(),
                        );
                    };
                    result.push_str(&self.numbered_placeholder(position + 1)?);
                }
                PlaceholderedSqlToken::FragmentPlaceholder(fragment) => {
                    return PlaceholderedSqlRenderError::unresolved_fragment(fragment.as_str());
                }
                PlaceholderedSqlToken::LiteralText(literal) => result.push_str(&literal.sql()),
                PlaceholderedSqlToken::RawSqlText(raw) => result.push_str(raw.text()),
            }
        }
        Ok(result)
    }

    fn numbered_placeholder(self, number: usize) -> Result<String, PlaceholderedSqlRenderError> {
        match self {
            Self::Postgres => Ok(format!("${number}")),
            Self::Sqlite => {
                #[cfg(feature = "sqlite")]
                {
                    Ok(format!("?{number}"))
                }
                #[cfg(not(feature = "sqlite"))]
                {
                    PlaceholderedSqlRenderError::sqlite_feature_required()
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::PostgresDialect;
    #[cfg(feature = "sqlite")]
    use crate::SqliteDialect;

    use ::sql_fun_core::PlaceholderedSql;

    use crate::placeholdered_sql::Dialect;

    #[rstest::rstest]
    #[case::zero_parameters("select 42", &[], "select 42")]
    #[case::type_hints_are_not_rendered(
        "select ${id:uuid}, ${name:text}",
        &["id", "name"],
        "select $1, $2"
    )]
    #[case::caller_order_overrides_first_use(
        "select ${second}, ${first}, ${second}",
        &["first", "unused", "second"],
        "select $3, $1, $3"
    )]
    #[case::literal_and_raw_sql_are_preserved(
        "select '${ignored}', ${value} -- ${comment}\n",
        &["value"],
        "select '${ignored}', $1 -- ${comment}\n"
    )]
    #[case::nested_block_comment_is_preserved(
        "/* outer ${ignored} /* nested ${also_ignored} */ still ${ignored} */ select ${value}",
        &["value"],
        "/* outer ${ignored} /* nested ${also_ignored} */ still ${ignored} */ select $1"
    )]
    #[case::untagged_dollar_quote_is_preserved(
        "select $$ ${not_a_parameter} $$, ${value}",
        &["value"],
        "select $$ ${not_a_parameter} $$, $1"
    )]
    #[case::tagged_dollar_quote_is_preserved(
        "select $tag$ ${not_a_parameter} $tag$, ${value}",
        &["value"],
        "select $tag$ ${not_a_parameter} $tag$, $1"
    )]
    #[case::quoted_identifier_is_preserved(
        "select \"${not_a_parameter}\", ${value}",
        &["value"],
        "select \"${not_a_parameter}\", $1"
    )]
    fn renders_postgres_placeholders(
        #[case] source: &str,
        #[case] parameter_order: &[&str],
        #[case] expected: &str,
    ) {
        let sql = PlaceholderedSql::from_query(source);
        let actual = Dialect::postgres()
            .render_placeholdered_sql(&sql, parameter_order)
            .expect("rendered PostgreSQL SQL");
        assert_eq!(actual, expected);
        PostgresDialect::parse(&actual).expect("valid PostgreSQL SQL");
    }

    #[test]
    fn rejects_parameter_missing_from_caller_order() {
        let sql = PlaceholderedSql::from_query("select ${missing}");
        let error = Dialect::postgres()
            .render_placeholdered_sql(&sql, &["known"])
            .expect_err("missing parameter must fail");
        assert_eq!(
            error.to_string(),
            "query parameter missing is absent from the supplied parameter ordering"
        );
    }

    #[test]
    fn accepts_core_parameter_names_as_the_caller_order() {
        let sql = PlaceholderedSql::from_query("select ${account_id}, ${user_id}");
        let parameter_order = [
            ::sql_fun_core::ParameterName::from("user_id"),
            ::sql_fun_core::ParameterName::from("account_id"),
        ];

        let actual = Dialect::postgres()
            .render_placeholdered_sql(&sql, &parameter_order)
            .expect("rendered PostgreSQL SQL");
        assert_eq!(actual, "select $2, $1");
    }

    #[test]
    fn rejects_unresolved_fragment() {
        let sql = PlaceholderedSql::from_query("select * from users #[condition]");
        let error = Dialect::postgres()
            .render_placeholdered_sql(&sql, &[] as &[&str])
            .expect_err("unresolved fragment must fail");
        assert_eq!(
            error.to_string(),
            "unresolved SQL fragment placeholder condition"
        );
    }

    #[test]
    fn reuses_caller_order_for_independent_query_fragments() {
        let parameter_order = ["account_id", "user_id", "unused"];
        let base_query = PlaceholderedSql::from_query("select * from users where id = ${user_id}");
        let cte_query = PlaceholderedSql::from_query(
            "select * from accounts where id = ${account_id} and owner_id = ${user_id}",
        );

        assert_eq!(
            Dialect::postgres()
                .render_placeholdered_sql(&base_query, &parameter_order)
                .expect("rendered base query"),
            "select * from users where id = $2"
        );
        assert_eq!(
            Dialect::postgres()
                .render_placeholdered_sql(&cte_query, &parameter_order)
                .expect("rendered CTE query"),
            "select * from accounts where id = $1 and owner_id = $2"
        );
    }

    #[cfg(feature = "sqlite")]
    #[rstest::rstest]
    #[case::contiguous("select ${first}, ${second}", &["first", "second"], "select ?1, ?2")]
    #[case::gapped("select ${third}, ${first}", &["first", "unused", "third"], "select ?3, ?1")]
    fn renders_sqlite_placeholders(
        #[case] source: &str,
        #[case] parameter_order: &[&str],
        #[case] expected: &str,
    ) {
        let sql = PlaceholderedSql::from_query(source);
        let actual = Dialect::sqlite()
            .render_placeholdered_sql(&sql, parameter_order)
            .expect("rendered SQLite SQL");
        assert_eq!(actual, expected);
        SqliteDialect::parse(&actual).expect("valid SQLite SQL");
    }
}
