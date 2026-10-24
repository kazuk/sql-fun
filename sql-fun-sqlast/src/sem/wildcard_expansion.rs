//! Wildcard projection groups captured during the existing semantic resolution.

use crate::sem::{ResultColumn, ResultColumnCollection};

/// The ordered semantic output of one source wildcard projection.
///
/// Lexical offsets are relative to the parser input for the owning statement.
/// They are absent for dialects whose parser does not supply target locations.
/// The group is transient analysis output and does not change serialized columns.
#[derive(Clone, Debug)]
pub struct WildcardExpansion {
    location: Option<usize>,
    columns: Vec<ResultColumn>,
    table_dependent: bool,
    references: Vec<Option<Vec<String>>>,
    source_projections: Vec<WildcardSourceProjection>,
    source_ordinals: Vec<Option<(String, usize)>>,
    qualifiers: Vec<WildcardQualifier>,
    typed_records: Vec<Option<WildcardTypedRecord>>,
    rows_sources: Vec<WildcardRowsSource>,
}

impl WildcardExpansion {
    pub(crate) fn new(
        location: Option<usize>,
        results: &ResultColumnCollection,
        first: usize,
        table_dependent: bool,
        references: &[Option<Vec<String>>],
    ) -> Self {
        Self {
            location,
            columns: (first..results.len())
                .filter_map(|index| results.get_column_at(index).cloned())
                .collect(),
            table_dependent,
            references: references.to_vec(),
            source_projections: Vec::new(),
            source_ordinals: Vec::new(),
            qualifiers: Vec::new(),
            typed_records: Vec::new(),
            rows_sources: Vec::new(),
        }
    }

    /// Parser location of the original projection target, when supplied.
    pub const fn location(&self) -> Option<usize> {
        self.location
    }

    /// Resolved columns in their original projection order, including duplicates.
    pub fn columns(&self) -> &[ResultColumn] {
        &self.columns
    }

    pub(crate) fn reference_at(&self, index: usize) -> Option<&[String]> {
        self.references.get(index)?.as_deref()
    }

    pub(crate) fn retain_source_projections(&mut self, projections: &[WildcardSourceProjection]) {
        self.source_projections = projections.to_vec();
    }

    pub(crate) fn typed_record_at(&self, index: usize) -> Option<&WildcardTypedRecord> {
        self.typed_records.get(index)?.as_ref()
    }
    pub(crate) fn qualifiers(&self) -> &[WildcardQualifier] {
        &self.qualifiers
    }

    pub(crate) fn rows_sources(&self) -> &[WildcardRowsSource] {
        &self.rows_sources
    }
    pub(crate) fn retain_projection_origins(
        &mut self,
        columns: &[crate::sem::ProjectionColumn<'_>],
    ) {
        for column in columns {
            column.collect_rows_sources(&mut self.rows_sources);
        }
        self.typed_records = columns.iter().map(|column| column.typed_record()).collect();
        for column in columns {
            column.collect_qualifiers(&mut self.qualifiers);
        }
        self.source_ordinals = columns
            .iter()
            .map(|column| {
                column
                    .source_ordinal()
                    .map(|(alias, ordinal)| (alias.to_owned(), ordinal))
            })
            .collect();
    }
    pub(crate) fn source_ordinal_at(&self, index: usize) -> Option<(&str, usize)> {
        self.source_ordinals
            .get(index)?
            .as_ref()
            .map(|(alias, ordinal)| (alias.as_str(), *ordinal))
    }

    pub(crate) fn source_projections(&self) -> &[WildcardSourceProjection] {
        &self.source_projections
    }

    /// Whether this wildcard enumerates a mutable table column set at its level.
    pub const fn is_table_dependent(&self) -> bool {
        self.table_dependent
    }
}

/// An aliased source whose duplicate output labels require a fixed inner projection.
#[derive(Clone, Debug)]
pub(crate) struct WildcardSourceProjection {
    alias: String,
    columns: Vec<ResultColumn>,
    references: Vec<Option<Vec<String>>>,
    qualifiers: Vec<WildcardQualifier>,
    source_ordinals: Vec<Option<(String, usize)>>,
    source_projections: Vec<Self>,
    typed_records: Vec<Option<WildcardTypedRecord>>,
    rows_sources: Vec<WildcardRowsSource>,
}

impl WildcardSourceProjection {
    pub(crate) fn from_columns(
        source_columns: &[crate::sem::ProjectionColumn<'_>],
        context: &mut impl crate::sem::ParseContext,
        scope: &crate::sem::ScalarAnalysisContext<'_>,
    ) -> Result<Vec<Self>, crate::sem::AnalysisError> {
        crate::sem::with_stack(|| {
            let mut sources = Vec::new();
            for column in source_columns {
                column.collect_projection_sources(&mut sources);
            }
            let mut projections = Vec::new();
            for source in sources {
                let mut columns = ResultColumnCollection::default();
                let mut references = Vec::new();
                let mut qualifiers = Vec::new();
                // These bindings already identify exact source occurrences. Building
                // their expressions does not repeat alias or column-name resolution.
                for column in source.columns() {
                    column.collect_qualifiers(&mut qualifiers);
                    columns.push_expression(column.name(), column.bind(context, scope)?);
                    references.push(column.visible_reference());
                }
                projections.push(Self {
                    alias: source.alias().as_str().to_owned(),
                    columns: (0..columns.len())
                        .map(|index| columns.get_column_at(index).unwrap().clone())
                        .collect(),
                    references,
                    qualifiers,
                    source_ordinals: source
                        .columns()
                        .iter()
                        .map(|column| {
                            column
                                .source_ordinal()
                                .map(|(alias, ordinal)| (alias.to_owned(), ordinal))
                        })
                        .collect(),
                    source_projections: Self::from_columns(source.columns(), context, scope)?,
                    typed_records: source
                        .columns()
                        .iter()
                        .map(|column| column.typed_record())
                        .collect(),
                    rows_sources: {
                        let mut rows = Vec::new();
                        for column in source.columns() {
                            column.collect_rows_sources(&mut rows);
                        }
                        rows
                    },
                });
            }
            Ok(projections)
        })
    }
    pub(crate) fn rows_sources(&self) -> &[WildcardRowsSource] {
        &self.rows_sources
    }
    pub(crate) fn typed_record_at(&self, index: usize) -> Option<&WildcardTypedRecord> {
        self.typed_records.get(index)?.as_ref()
    }
    pub(crate) fn source_projections(&self) -> &[Self] {
        &self.source_projections
    }
    pub(crate) fn source_ordinal_at(&self, index: usize) -> Option<(&str, usize)> {
        self.source_ordinals
            .get(index)?
            .as_ref()
            .map(|(alias, ordinal)| (alias.as_str(), *ordinal))
    }
    pub(crate) fn qualifiers(&self) -> &[WildcardQualifier] {
        &self.qualifiers
    }
    pub(crate) fn alias(&self) -> &str {
        &self.alias
    }
    pub(crate) fn columns(&self) -> &[ResultColumn] {
        &self.columns
    }
    pub(crate) fn reference_at(&self, index: usize) -> Option<&[String]> {
        self.references.get(index)?.as_deref()
    }
}

/// An already resolved relation occurrence's legal SQL qualifier.
#[derive(Clone, Debug)]
pub(crate) struct WildcardQualifier {
    relation: crate::sem::FullName,
    alias: crate::sem::AliasName,
    parts: Vec<String>,
}

impl WildcardQualifier {
    pub(crate) fn new(
        relation: &crate::sem::FullName,
        alias: &crate::sem::AliasName,
        parts: &[String],
    ) -> Self {
        Self {
            relation: relation.clone(),
            alias: alias.clone(),
            parts: parts.to_vec(),
        }
    }
    pub(crate) fn relation(&self) -> &crate::sem::FullName {
        &self.relation
    }
    pub(crate) fn alias(&self) -> &crate::sem::AliasName {
        &self.alias
    }
    pub(crate) fn parts(&self) -> &[String] {
        &self.parts
    }
}

/// Positional base-relation output whose duplicate alias labels cannot be named.
#[derive(Clone, Debug)]
pub(crate) struct WildcardTypedRecord {
    alias: String,
    relation: crate::sem::FullName,
    column: String,
}

impl WildcardTypedRecord {
    pub(crate) fn new(
        alias: &crate::sem::AliasName,
        relation: &crate::sem::FullName,
        column: &crate::sem::ColumnName,
    ) -> Self {
        Self {
            alias: alias.as_str().to_owned(),
            relation: relation.clone(),
            column: column.as_str().to_owned(),
        }
    }
    pub(crate) fn alias(&self) -> &str {
        &self.alias
    }
    pub(crate) fn relation(&self) -> &crate::sem::FullName {
        &self.relation
    }
    pub(crate) fn column(&self) -> &str {
        &self.column
    }
}

/// Resolved output layout of a ROWS FROM invocation before outer aliasing.
#[derive(Clone, Debug)]
pub(crate) struct WildcardRowsSource {
    alias: String,
    functions: Vec<WildcardRowsFunction>,
    output_names: Vec<String>,
    ordinality: bool,
}

impl WildcardRowsSource {
    pub(crate) fn new(
        alias: &crate::sem::AliasName,
        functions: &[WildcardRowsFunction],
        output_names: &[crate::sem::ColumnName],
        ordinality: bool,
    ) -> Self {
        Self {
            alias: alias.as_str().to_owned(),
            functions: functions.to_vec(),
            output_names: output_names
                .iter()
                .map(|name| name.as_str().to_owned())
                .collect(),
            ordinality,
        }
    }
    pub(crate) fn alias(&self) -> &str {
        &self.alias
    }
    pub(crate) fn functions(&self) -> &[WildcardRowsFunction] {
        &self.functions
    }
    pub(crate) fn output_names(&self) -> &[String] {
        &self.output_names
    }
    pub(crate) const fn has_ordinality(&self) -> bool {
        self.ordinality
    }
}

#[derive(Clone, Debug)]
pub(crate) struct WildcardRowsFunction {
    columns: Vec<String>,
}
impl WildcardRowsFunction {
    pub(crate) fn from_columns(columns: &[crate::sem::ColumnName]) -> Self {
        Self {
            columns: columns
                .iter()
                .map(|name| name.as_str().to_owned())
                .collect(),
        }
    }
    pub(crate) fn columns(&self) -> &[String] {
        &self.columns
    }
}
