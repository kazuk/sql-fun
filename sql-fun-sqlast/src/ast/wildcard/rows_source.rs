//! Freeze a combined function source without changing its zip or row interface.

use crate::sem::WildcardRowsProjection;

use crate::{
    ast::{
        Call, Cte, DerivedTable, Expr, FromItem, Join, Name, Query, Relation, TableFunctions,
        Target, wildcard::WildcardRewriteError,
    },
    sem::WildcardRowsSource,
};

/// Keep each invocation once, number its rows, and zip the fixed outputs by row.
/// All helper fields stay inside the returned derived source.
pub(super) fn freeze(
    resolved: &WildcardRowsSource,
    source: TableFunctions,
) -> Result<FromItem, WildcardRewriteError> {
    if !source.has_invocations(resolved.function_count()) {
        return Err(WildcardRewriteError::invalid(
            "ROWS FROM output layout does not match its original function invocations",
        ));
    }
    if !resolved.has_valid_output_layout() {
        return Err(WildcardRewriteError::invalid(
            "ROWS FROM output labels do not match its fields",
        ));
    }
    let mut builder = RowsProjectionBuilder::new(&source, resolved)?;
    resolved
        .try_project_functions(|index, projection| builder.freeze_function(index, projection))?;
    Ok(builder.finish(resolved))
}

#[cfg(test)]
mod test_freeze {
    use crate::ast::wildcard::rows_source::freeze;

    use crate::test_helpers::wildcard_rows_source::invocation;

    use crate::test_helpers::wildcard_rows_source::layout;

    use crate::test_helpers::wildcard_rows_source::rendered;

    #[rstest::rstest]
    #[case::plain(false)]
    #[case::ordinality(true)]
    fn zips_fixed_function_projections_without_exposing_helper_columns(#[case] ordinality: bool) {
        let suffix = if ordinality { " WITH ORDINALITY" } else { "" };
        let source = invocation(&format!(
            "SELECT * FROM ROWS FROM (generate_series(1,2),generate_series(10,13)){suffix} AS r"
        ));
        let outputs: &[&str] = if ordinality {
            &["id", "id", "position"]
        } else {
            &["id", "id"]
        };
        let resolved = layout(
            &[&["generate_series"], &["generate_series"]],
            outputs,
            ordinality,
        );
        let frozen = freeze(&resolved, source).unwrap();
        let sql = rendered(frozen);
        assert!(sql.ends_with(r#") AS "r""#), "{sql}");
        ::pg_query::parse(&sql, ::pg_query::ParserOptions::default()).unwrap();
        assert_eq!(
            sql.matches(r#""generate_series"(1, 2)"#).count(),
            1,
            "{sql}"
        );
        assert_eq!(
            sql.matches(r#""generate_series"(10, 13)"#).count(),
            1,
            "{sql}"
        );
        assert_eq!(sql.matches(r#""row_number"()"#).count(), 2, "{sql}");
        assert!(sql.contains("FULL JOIN"), "{sql}");
        assert_eq!(sql.matches("AS MATERIALIZED").count(), 2, "{sql}");
        assert_eq!(sql.matches(r#""count"(*)"#).count(), 1, "{sql}");
        assert!(sql.contains(r#"OPERATOR("pg_catalog".>=)"#), "{sql}");
        assert!(sql.contains("ORDER BY"), "{sql}");
    }

    #[test]
    fn retains_per_function_record_definitions() {
        let source = invocation(
            r#"SELECT * FROM ROWS FROM (json_to_recordset('[{"id":1}]') AS (id int4),generate_series(1,2)) AS r"#,
        );
        let resolved = layout(&[&["id"], &["generate_series"]], &["id", "id"], false);
        let sql = rendered(freeze(&resolved, source).unwrap());
        ::pg_query::parse(&sql, ::pg_query::ParserOptions::default()).unwrap();
        assert!(sql.contains(r#"AS ("id" "int4")"#), "{sql}");
    }

    #[rstest::rstest]
    #[case::missing_function(&[])]
    #[case::missing_output(&[&["generate_series"][..]][..])]
    fn mismatched_layout_rejects_before_building_a_source(#[case] functions: &[&[&str]]) {
        let source = invocation("SELECT * FROM ROWS FROM (generate_series(1,2)) AS r");
        let resolved = layout(functions, &[], false);
        assert!(
            freeze(&resolved, source)
                .unwrap_err()
                .is_invalid_expression()
        );
    }
}

struct RowsProjectionBuilder<'a> {
    source: &'a TableFunctions,
    names: PrivateNames,
    ordinal: String,
    zipped: Option<FromItem>,
    entries: Vec<Cte>,
    previous: Option<String>,
    targets: Vec<Target>,
}

impl<'a> RowsProjectionBuilder<'a> {
    fn new(
        source: &'a TableFunctions,
        resolved: &WildcardRowsSource,
    ) -> Result<Self, WildcardRewriteError> {
        let mut names = PrivateNames::new(source, resolved)?;
        let ordinal = names.next();
        Ok(Self {
            source,
            names,
            ordinal,
            zipped: None,
            entries: Vec::new(),
            previous: None,
            targets: Vec::new(),
        })
    }

    fn freeze_function(
        &mut self,
        index: usize,
        projection: &WildcardRowsProjection<'_>,
    ) -> Result<(), WildcardRewriteError> {
        let alias = self.names.next();
        // Scalar default output labels follow the source alias. The known single
        // label also works for one-column composites without a column alias list.
        let input_alias = if let Some(column) = projection.singleton_column_name() {
            column.to_owned()
        } else {
            self.names.next()
        };
        let Some(input) = self.source.isolated_invocation(index, &input_alias) else {
            return Err(WildcardRewriteError::invalid(
                "ROWS FROM invocation is missing",
            ));
        };
        let fields = self.project_fields(projection, &input_alias, &alias);
        let mut fixed = Query::projection(&fields, &[FromItem::functions(input)]);
        if let Some(prior) = &self.previous {
            fixed.after_materialized_source(prior);
        }
        self.entries.push(Cte::materialized_query(&alias, &fixed));
        self.previous = Some(alias.clone());
        let individual = FromItem::relation(Relation::local(&alias));
        self.zipped = Some(if let Some(left) = self.zipped.take() {
            FromItem::join(Join::full_using(&left, &individual, &self.ordinal))
        } else {
            individual
        });
        Ok(())
    }

    fn project_fields(
        &mut self,
        projection: &WildcardRowsProjection<'_>,
        input_alias: &str,
        output_alias: &str,
    ) -> Vec<Target> {
        let mut fields = Vec::new();
        projection.project_columns(|column, output| {
            self.project_field(&mut fields, input_alias, output_alias, column, output)
        });
        fields.push(Target::projection(
            &Expr::projection_call(&Call::row_number()),
            &self.ordinal,
        ));
        fields
    }

    fn project_field(
        &mut self,
        fields: &mut Vec<Target>,
        input_alias: &str,
        output_alias: &str,
        column: &str,
        output: &str,
    ) {
        fields.push(Target::projection(&reference(input_alias, column), column));
        self.targets
            .push(Target::projection(&reference(output_alias, column), output));
    }

    fn finish(mut self, resolved: &WildcardRowsSource) -> FromItem {
        if let Some(output) = resolved.ordinality_name() {
            self.targets.push(Target::projection(
                &Expr::projection_column(&Name::local(&self.ordinal)),
                output,
            ));
        }
        let mut query = Query::projection(&self.targets, &[self.zipped.expect("nonempty calls")]);
        query.order_materialized_sources(&self.entries, &self.ordinal);
        FromItem::query(DerivedTable::lateral_projection(&query, resolved.alias()))
    }
}

fn reference(alias: &str, column: &str) -> Expr {
    Expr::projection_column(&Name::from_parts(&[alias.to_owned(), column.to_owned()]))
}

/// Generated names are ASCII, so literal containment conservatively excludes
/// every spelling appearing in the original deparsed invocation or output names.
/// This only allocates private names; it never recovers locations or SQL facts.
struct PrivateNames {
    reserved: String,
    next: usize,
}

impl PrivateNames {
    fn new(
        source: &TableFunctions,
        resolved: &WildcardRowsSource,
    ) -> Result<Self, WildcardRewriteError> {
        let mut reserved = source
            .rendered_invocation()
            .map_err(|error| WildcardRewriteError::invalid(&error.to_string()))?;
        resolved.visit_names(|column| {
            reserved.push('\n');
            reserved.push_str(column);
        });
        Ok(Self { reserved, next: 0 })
    }

    fn next(&mut self) -> String {
        loop {
            let candidate = format!("__sql_fun_rows_{}", self.next);
            self.next += 1;
            if !self.reserved.contains(&candidate) {
                self.reserved.push('\n');
                self.reserved.push_str(&candidate);
                return candidate;
            }
        }
    }
}

#[cfg(test)]
mod test_private_names {
    use crate::ast::wildcard::rows_source::PrivateNames;

    use crate::test_helpers::wildcard_rows_source::invocation;

    use crate::test_helpers::wildcard_rows_source::layout;

    #[test]
    fn private_names_do_not_shadow_correlated_bindings_or_output_columns() {
        let source =
            invocation("SELECT * FROM ROWS FROM (generate_series(1,__sql_fun_rows_0.id)) AS r");
        let resolved = layout(&[&["__sql_fun_rows_1"]], &["__sql_fun_rows_2"], false);
        let mut names = PrivateNames::new(&source, &resolved).unwrap();
        assert_eq!(names.next(), "__sql_fun_rows_3");
        assert_eq!(names.next(), "__sql_fun_rows_4");
    }
}
