use crate::{
    ast::{
        DerivedTable, Expr, FromItem, Name, Query, Statement, Target, TypeName,
        projection::{ProjectionScope, ProjectionScopes, ScopedSource, ScopedSourceEdit},
    },
    sem::{
        WildcardExpansion, WildcardRewriteError, WildcardRowsSource, WildcardSourceProjection,
        WildcardTypedRecord, wildcard_expansion::rewrite::rows_source::freeze,
    },
    stack::with_stack,
};
// Stage resolved source transformations and commit them in lexical scope order.

pub(super) struct SourcePlans {
    edits: Vec<SourceEdit>,
    failure: Option<WildcardRewriteError>,
}
impl SourcePlans {
    pub(super) const fn new() -> Self {
        Self {
            edits: Vec::new(),
            failure: None,
        }
    }
    fn add(&mut self, source: &ScopedSource, alias: &str, transform: &SourceTransform) {
        if self
            .edits
            .iter()
            .any(|edit| edit.belongs_to(source.scope(), alias))
        {
            return;
        }
        self.edits
            .push(SourceEdit::new(source.scope(), alias, transform));
    }
    pub(super) fn stage(&mut self, statement: &mut Statement) -> Result<(), WildcardRewriteError> {
        statement.apply_source_edits(self);
        if let Some(error) = self.failure.take() {
            Err(error)
        } else {
            Ok(())
        }
    }
}
impl ScopedSourceEdit for SourcePlans {
    fn replace_sources(&mut self, scope: usize, projection: &mut ProjectionScope<'_>) {
        for edit in &self.edits {
            if self.failure.is_none() {
                self.failure = edit.apply_in_scope(scope, projection).err();
            }
        }
    }
}
struct SourceEdit {
    owner: usize,
    alias: String,
    transform: SourceTransform,
}
impl SourceEdit {
    fn new(owner: usize, alias: &str, transform: &SourceTransform) -> Self {
        Self {
            owner,
            alias: alias.to_owned(),
            transform: transform.clone(),
        }
    }
    fn belongs_to(&self, owner: usize, alias: &str) -> bool {
        self.owner == owner && self.alias == alias
    }
    fn apply_in_scope(
        &self,
        owner: usize,
        scope: &mut ProjectionScope<'_>,
    ) -> Result<(), WildcardRewriteError> {
        if self.owner != owner {
            return Ok(());
        }
        // This runs only on the isolated staging statement. Rebuilding from its
        // current subtree retains successfully staged child-scope edits.
        scope.try_transform_source(&self.alias, &mut |source| self.transform.apply(source))
    }
}
#[derive(Clone)]
enum SourceTransform {
    Projection(WildcardSourceProjection),
    Rows(WildcardRowsSource),
}
impl SourceTransform {
    fn projection(source: &WildcardSourceProjection) -> Self {
        Self::Projection(source.clone())
    }
    fn rows(source: &WildcardRowsSource) -> Self {
        Self::Rows(source.clone())
    }
    fn apply(&self, source: &FromItem) -> Result<FromItem, WildcardRewriteError> {
        match self {
            Self::Projection(projection) => projection.freeze_source(source),
            Self::Rows(rows) => rows.freeze_rows_source(source),
        }
    }
}
impl WildcardExpansion {
    pub(super) fn prepare_sources(
        &self,
        scopes: &ProjectionScopes,
        plans: &mut SourcePlans,
    ) -> Result<(), WildcardRewriteError> {
        let Some(location) = self.location() else {
            return Err(WildcardRewriteError::missing_location());
        };
        for rows in self.rows_sources() {
            let source = scopes
                .source_for_target(location, self.source_scope_depth(), rows.alias())
                .map_err(WildcardRewriteError::from)?;
            plans.add(&source, rows.alias(), &SourceTransform::rows(rows));
        }
        for projection in self.source_projections() {
            let source = scopes
                .source_for_target(location, self.source_scope_depth(), projection.alias())
                .map_err(WildcardRewriteError::from)?;
            plans.add(
                &source,
                projection.alias(),
                &SourceTransform::projection(projection),
            );
        }
        Ok(())
    }
}
impl WildcardSourceProjection {
    fn freeze_source(&self, source: &FromItem) -> Result<FromItem, WildcardRewriteError> {
        with_stack(|| {
            let mut from = source.clone();
            from.clear_projection_alias();
            for rows in self.rows_sources() {
                let nested = from
                    .visible_source(rows.alias())
                    .map_err(WildcardRewriteError::from)?;
                let replacement = rows.freeze_rows_source(&nested)?;
                from.replace_visible_source(rows.alias(), &replacement);
            }
            for projection in self.source_projections() {
                let nested = from
                    .visible_source(projection.alias())
                    .map_err(WildcardRewriteError::from)?;
                let replacement = projection.freeze_source(&nested)?;
                from.replace_visible_source(projection.alias(), &replacement);
            }
            let targets = self.rewrite_targets()?;
            let query = Query::projection(&targets, &[from]);
            Ok(FromItem::query(DerivedTable::lateral_projection(
                &query,
                self.alias(),
            )))
        })
    }
    fn rewrite_targets(&self) -> Result<Vec<Target>, WildcardRewriteError> {
        let bindings = self.qualifiers();
        let mut targets = Vec::new();
        for (index, column) in self.columns().iter().enumerate() {
            let expression = if let Some(record) = self.typed_record_at(index) {
                typed_record_reference(record)
            } else if let Some((alias, ordinal)) = self.source_ordinal_at(index) {
                ordinal_reference(alias, ordinal)
            } else if let Some(parts) = self.reference_at(index) {
                Expr::projection_column(&Name::from_parts(parts))
            } else {
                let Some(expression) = column.expression() else {
                    return Err(WildcardRewriteError::invalid(
                        "duplicate-label source has no resolved expression",
                    ));
                };
                expression.projection_expression(bindings)?
            };
            let Some(name) = column.column_name() else {
                return Err(WildcardRewriteError::invalid(
                    "duplicate-label source has no output name",
                ));
            };
            targets.push(Target::projection(&expression, name.as_str()));
        }
        Ok(targets)
    }
}
pub(super) fn typed_record_reference(record: &WildcardTypedRecord) -> Expr {
    let target = TypeName::projection(&Name::from_full_name(record.relation()), false);
    let row = Expr::projection_source_row(record.alias());
    let cast = Expr::projection_cast(&row, &target);
    Expr::projection_field(&cast, record.column())
}
pub(super) fn ordinal_reference(alias: &str, ordinal: usize) -> Expr {
    Expr::projection_field(
        &Expr::projection_source_row(alias),
        &format!("f{}", ordinal + 1),
    )
}
impl WildcardRowsSource {
    fn freeze_rows_source(&self, source: &FromItem) -> Result<FromItem, WildcardRewriteError> {
        match source {
            FromItem::Functions(functions) => freeze(self, functions.clone()),
            _ => Err(WildcardRewriteError::invalid(
                "ROWS FROM origin is not a function source",
            )),
        }
    }
}
