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

#[cfg(test)]
mod test_helpers;
#[cfg(test)]
mod tests;

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
mod private_names_tests;
