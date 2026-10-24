//! Fix source projections that cannot be expressed by stable qualified names.

use crate::ast::wildcard::{VisitTargets, WildcardRewriteError};
use crate::ast::{
    DerivedTable, Expr, FromItem, Name, Query, QueryBody, Select, Statement, Target, With,
};
use crate::sem::{WildcardExpansion, WildcardSourceProjection};

pub(super) struct SourcePlans {
    edits: Vec<SourceEdit>,
}

struct SourceEdit {
    owner: usize,
    alias: String,
    replacement: FromItem,
}

impl SourcePlans {
    pub(super) fn prepare(
        statement: &mut Statement,
        expansions: &[WildcardExpansion],
    ) -> Result<Self, WildcardRewriteError> {
        let mut edits: Vec<SourceEdit> = Vec::new();
        for expansion in expansions.iter().filter(|expansion| {
            expansion.is_table_dependent()
                && (!expansion.source_projections().is_empty()
                    || !expansion.rows_sources().is_empty())
        }) {
            let mut owner = None;
            let mut sources = Vec::new();
            statement
                .node
                .visit_targets(&mut |targets, _, source_items| {
                    if targets
                        .iter()
                        .any(|target| target.location == expansion.location())
                    {
                        owner = targets.iter().filter_map(|target| target.location).min();
                        sources = source_items.to_vec();
                    }
                });
            for resolved in expansion.rows_sources() {
                let owner = owner.ok_or_else(WildcardRewriteError::not_found)?;
                if edits
                    .iter()
                    .any(|edit| edit.owner == owner && edit.alias == resolved.alias())
                {
                    continue;
                }
                let mut matched = Vec::new();
                for source in &mut sources {
                    source.find_projection_source(resolved.alias(), &mut |source| {
                        matched.push(source.clone());
                    });
                }
                if matched.len() != 1 {
                    return Err(WildcardRewriteError::invalid(
                        "ROWS FROM source is missing or ambiguous",
                    ));
                }
                let replacement = matched.remove(0).freeze_rows_source(resolved)?;
                edits.push(SourceEdit {
                    owner,
                    alias: resolved.alias().to_owned(),
                    replacement,
                });
            }
            for projection in expansion.source_projections() {
                let owner = owner.ok_or_else(WildcardRewriteError::not_found)?;
                if edits
                    .iter()
                    .any(|edit| edit.owner == owner && edit.alias == projection.alias())
                {
                    continue;
                }
                let mut original = None;
                let mut count = 0;
                for source in &mut sources {
                    source.find_projection_source(projection.alias(), &mut |source| {
                        count += 1;
                        original = Some(source.clone());
                    });
                }
                if count != 1 {
                    return Err(WildcardRewriteError::invalid(
                        "duplicate-label source is missing or ambiguous in the wildcard scope",
                    ));
                }
                let mut original = original.expect("one source matched");
                original.clear_projection_alias();
                let replacement = derived(projection, original)?;
                edits.push(SourceEdit {
                    owner,
                    alias: projection.alias().to_owned(),
                    replacement,
                });
            }
        }
        Ok(Self { edits })
    }

    pub(super) fn apply(&self, targets: &[Target], sources: &mut [FromItem]) {
        let owner = targets.iter().filter_map(|target| target.location).min();
        for edit in &self.edits {
            if owner != Some(edit.owner) {
                continue;
            }
            for source in sources.iter_mut() {
                source.find_projection_source(&edit.alias, &mut |source| {
                    *source = edit.replacement.clone();
                });
                source.normalize_projection_groups();
            }
        }
    }
}

fn freeze_nested_sources(
    source: &mut FromItem,
    projections: &[WildcardSourceProjection],
) -> Result<(), WildcardRewriteError> {
    for projection in projections {
        let mut failure = None;
        let mut matches = 0;
        source.find_projection_source(projection.alias(), &mut |source| {
            matches += 1;
            let mut original = source.clone();
            original.clear_projection_alias();
            match derived(projection, original) {
                Ok(replacement) => *source = replacement,
                Err(error) => failure = Some(error),
            }
        });
        if let Some(error) = failure {
            return Err(error);
        }
        if matches != 1 {
            return Err(WildcardRewriteError::invalid(
                "nested duplicate-label source is missing or ambiguous",
            ));
        }
        source.normalize_projection_groups();
    }
    Ok(())
}

pub(super) fn typed_record_reference(record: &crate::sem::WildcardTypedRecord) -> Expr {
    let name = record.relation();
    let mut parts = Vec::new();
    if let Some(catalog) = name.catalog() {
        parts.push(catalog.to_string());
    }
    if let Some(schema) = name.schema() {
        parts.push(schema.to_string());
    }
    parts.push(name.local_name());
    let target = crate::ast::TypeName {
        name: Name::from_parts(&parts),
        modifiers: Vec::new(),
        array_bounds: Vec::new(),
        set_of: false,
        percent_type: false,
        sqlite_name_snapshot: None,
        sqlite_parts: Vec::new(),
        sqlite_modifiers: None,
        location: None,
    };
    Expr::projection_field(
        Expr::projection_cast_node(crate::ast::Cast {
            argument: Expr::projection_source_row(record.alias()),
            target,
        }),
        record.column().to_owned(),
    )
}

pub(super) fn ordinal_reference(alias: &str, ordinal: usize) -> Expr {
    Expr::projection_field(
        Expr::projection_source_row(alias),
        format!("f{}", ordinal + 1),
    )
}

fn derived(
    projection: &WildcardSourceProjection,
    mut from: FromItem,
) -> Result<FromItem, WildcardRewriteError> {
    crate::sem::with_stack(|| {
        for rows in projection.rows_sources() {
            let mut failure = None;
            let mut count = 0;
            from.find_projection_source(rows.alias(), &mut |source| {
                count += 1;
                match source.clone().freeze_rows_source(rows) {
                    Ok(replacement) => *source = replacement,
                    Err(error) => failure = Some(error),
                }
            });
            if let Some(error) = failure {
                return Err(error);
            }
            if count != 1 {
                return Err(WildcardRewriteError::invalid(
                    "nested ROWS FROM source is missing or ambiguous",
                ));
            }
        }
        freeze_nested_sources(&mut from, projection.source_projections())?;
        let mut targets = Vec::new();
        for (index, column) in projection.columns().iter().enumerate() {
            let expression = if let Some(record) = projection.typed_record_at(index) {
                typed_record_reference(record)
            } else if let Some((alias, ordinal)) = projection.source_ordinal_at(index) {
                ordinal_reference(alias, ordinal)
            } else if let Some(parts) = projection.reference_at(index) {
                Expr::projection_column(Name::from_parts(parts))
            } else {
                column
                    .expression()
                    .ok_or_else(|| {
                        WildcardRewriteError::invalid(
                            "duplicate-label source has no resolved expression",
                        )
                    })?
                    .projection_expression(projection.qualifiers())?
            };
            let name = column.column_name().as_ref().ok_or_else(|| {
                WildcardRewriteError::invalid("duplicate-label source has no output name")
            })?;
            targets.push(Target {
                location: None,
                expression,
                alias: Some(name.to_string()),
            });
        }
        Ok(FromItem::query(DerivedTable {
            query: Query {
                with: With::default(),
                body: QueryBody::projection_select(Select {
                    into: None,
                    distinct: None,
                    targets,
                    from: vec![from],
                    predicate: None,
                    group: Vec::new(),
                    group_distinct: false,
                    having: None,
                    windows: Vec::new(),
                }),
                order: Vec::new(),
                limit: None,
                offset: None,
                with_ties: false,
                locks: Vec::new(),
            },
            alias: Some(crate::ast::Alias {
                name: projection.alias().to_owned(),
                columns: Vec::new(),
            }),
            lateral: true,
        }))
    })
}

impl FromItem {
    fn freeze_rows_source(
        self,
        resolved: &crate::sem::WildcardRowsSource,
    ) -> Result<Self, WildcardRewriteError> {
        match self {
            Self::Functions(functions) => super::rows_source::freeze(resolved, functions),
            _ => Err(WildcardRewriteError::invalid(
                "ROWS FROM origin is not a function source",
            )),
        }
    }

    fn normalize_projection_groups(&mut self) {
        crate::sem::with_stack(|| match self {
            Self::Join(node) => {
                node.left.normalize_projection_groups();
                node.right.normalize_projection_groups();
            }
            Self::Group(node) => {
                for source in &mut node.items {
                    source.normalize_projection_groups();
                }
                if node.alias.is_none()
                    && node.items.len() == 1
                    && matches!(node.items[0], Self::Query(_))
                {
                    *self = node.items.remove(0);
                }
            }
            _ => {}
        })
    }

    fn find_projection_source(&mut self, alias: &str, visitor: &mut impl FnMut(&mut Self)) {
        crate::sem::with_stack(|| {
            let name = match self {
                Self::Join(node) => node.alias.as_ref().map(|alias| alias.name.as_str()),
                Self::Group(node) => node.alias.as_ref().map(|alias| alias.name.as_str()),
                Self::Functions(node) => node
                    .alias
                    .as_ref()
                    .map(|alias| alias.name.as_str())
                    .or_else(|| {
                        node.calls
                            .first()
                            .and_then(|call| call.expression.projection_function_alias())
                    }),
                _ => None,
            };
            if let Some(name) = name {
                if name == alias {
                    visitor(self);
                }
                return;
            }
            match self {
                Self::Join(node) => {
                    node.left.find_projection_source(alias, visitor);
                    node.right.find_projection_source(alias, visitor);
                }
                Self::Group(node) => {
                    for source in &mut node.items {
                        source.find_projection_source(alias, visitor);
                    }
                }
                _ => {}
            }
        })
    }

    fn clear_projection_alias(&mut self) {
        match self {
            Self::Join(node) => node.alias = None,
            Self::Group(node) => node.alias = None,
            _ => {}
        }
    }
}
