//! Wildcard projection groups captured during the existing semantic resolution.

mod rewrite;

pub use self::rewrite::WildcardRewriteError;

use crate::{
    names::{AliasName, ColumnName, FullName, TableName, ViewName},
    sem::{
        OverloadVariant, ParseContext, ProjectionColumn, ResultColumn, ResultColumnCollection,
        SemScalarExpr,
    },
};

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
    source_scope_depth: usize,
    references: Vec<Option<Vec<String>>>,
    source_projections: Vec<WildcardSourceProjection>,
    source_ordinals: Vec<Option<(String, usize)>>,
    qualifiers: WildcardQualifiers,
    typed_records: Vec<Option<WildcardTypedRecord>>,
    rows_sources: WildcardRowsSources,
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
            columns: results.cloned_suffix(first),
            table_dependent,
            source_scope_depth: 0,
            references: references.to_vec(),
            source_projections: Vec::new(),
            source_ordinals: Vec::new(),
            qualifiers: WildcardQualifiers::default(),
            typed_records: Vec::new(),
            rows_sources: WildcardRowsSources::default(),
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

    pub(crate) const fn source_scope_depth(&self) -> usize {
        self.source_scope_depth
    }
    pub(crate) const fn retain_source_scope(&mut self, depth: usize) {
        self.source_scope_depth = depth;
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
    pub(crate) const fn qualifiers(&self) -> &WildcardQualifiers {
        &self.qualifiers
    }

    pub(crate) fn rows_sources(&self) -> &[WildcardRowsSource] {
        self.rows_sources.sources()
    }
    pub(crate) fn retain_projection_origin(&mut self, column: &ProjectionColumn<'_>) {
        column.collect_rows_sources(&mut self.rows_sources);
        self.typed_records.push(column.typed_record());
        column.collect_qualifiers(&mut self.qualifiers);
        let ordinal = if let Some((alias, ordinal)) = column.source_ordinal() {
            Some((alias.to_owned(), ordinal))
        } else {
            None
        };
        self.source_ordinals.push(ordinal);
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
    qualifiers: WildcardQualifiers,
    source_ordinals: Vec<Option<(String, usize)>>,
    source_projections: Vec<Self>,
    typed_records: Vec<Option<WildcardTypedRecord>>,
    rows_sources: WildcardRowsSources,
}

impl WildcardSourceProjection {
    pub(crate) fn new(alias: &AliasName) -> Self {
        Self {
            alias: alias.as_str().to_owned(),
            columns: Vec::new(),
            references: Vec::new(),
            qualifiers: WildcardQualifiers::default(),
            source_ordinals: Vec::new(),
            source_projections: Vec::new(),
            typed_records: Vec::new(),
            rows_sources: WildcardRowsSources::default(),
        }
    }
    pub(crate) fn append_bound_column(
        &mut self,
        column: &ProjectionColumn<'_>,
        expression: &SemScalarExpr,
    ) {
        self.columns
            .push(ResultColumn::resolved(column.name(), expression));
        self.references.push(column.visible_reference());
        self.typed_records.push(column.typed_record());
        column.collect_qualifiers(&mut self.qualifiers);
        column.collect_rows_sources(&mut self.rows_sources);
        let ordinal = if let Some((alias, ordinal)) = column.source_ordinal() {
            Some((alias.to_owned(), ordinal))
        } else {
            None
        };
        self.source_ordinals.push(ordinal);
    }
    pub(crate) fn retain_source_projections(&mut self, projections: &[Self]) {
        self.source_projections = projections.to_vec();
    }
    pub(crate) fn rows_sources(&self) -> &[WildcardRowsSource] {
        self.rows_sources.sources()
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
    pub(crate) const fn qualifiers(&self) -> &WildcardQualifiers {
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

/// Canonical qualifiers retained for exact resolved source occurrences.
#[derive(Clone, Debug, Default)]
pub(crate) struct WildcardQualifiers(Vec<WildcardQualifier>);
impl WildcardQualifiers {
    pub(crate) fn include(&mut self, qualifier: &WildcardQualifier) {
        if !self
            .0
            .iter()
            .any(|existing| existing.is_for(qualifier.relation(), qualifier.alias()))
        {
            self.0.push(qualifier.clone());
        }
    }
    pub(crate) fn reference_for(
        &self,
        relation: &FullName,
        alias: &AliasName,
    ) -> Option<&[String]> {
        self.0
            .iter()
            .find(|qualifier| qualifier.is_for(relation, alias))
            .map(WildcardQualifier::parts)
    }
}

/// Resolved ROWS FROM occurrences, deduplicated in one projection scope.
#[derive(Clone, Debug, Default)]
pub(crate) struct WildcardRowsSources(Vec<WildcardRowsSource>);
impl WildcardRowsSources {
    pub(crate) fn include(&mut self, source: &WildcardRowsSource) {
        if !self
            .0
            .iter()
            .any(|existing| existing.alias() == source.alias())
        {
            self.0.push(source.clone());
        }
    }
    fn sources(&self) -> &[WildcardRowsSource] {
        &self.0
    }
}

/// An already resolved relation occurrence's legal SQL qualifier.
#[derive(Clone, Debug)]
pub(crate) struct WildcardQualifier {
    relation: FullName,
    alias: AliasName,
    parts: Vec<String>,
}

impl WildcardQualifier {
    fn is_for(&self, relation: &FullName, alias: &AliasName) -> bool {
        &self.relation == relation && &self.alias == alias
    }

    pub(crate) fn new(relation: &FullName, alias: &AliasName, parts: &[String]) -> Self {
        Self {
            relation: relation.clone(),
            alias: alias.clone(),
            parts: parts.to_vec(),
        }
    }
    pub(crate) const fn relation(&self) -> &FullName {
        &self.relation
    }
    pub(crate) const fn alias(&self) -> &AliasName {
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
    relation: FullName,
    column: String,
}

impl WildcardTypedRecord {
    pub(crate) fn from_table(
        alias: &AliasName,
        table: &TableName,
        context: &impl ParseContext,
        index: usize,
    ) -> Option<Self> {
        let column = context
            .get_table(table)?
            .get_column_at(index)?
            .name()
            .as_ref()?;
        Some(Self::new(alias, table.full_name(), column))
    }

    pub(crate) fn from_view(
        alias: &AliasName,
        view: &ViewName,
        context: &impl ParseContext,
        index: usize,
    ) -> Option<Self> {
        let column = context
            .get_view(view)?
            .result_columns()
            .get_column_at(index)?
            .column_name()
            .as_ref()?;
        Some(Self::new(alias, view.full_name(), column))
    }

    pub(crate) fn from_function(
        alias: &AliasName,
        function: &OverloadVariant,
        context: &impl ParseContext,
        index: usize,
    ) -> Option<Self> {
        let table = function
            .ret_type()
            .as_ref()?
            .definition_from_catalog(context)?
            .row_table_name(context)?;
        Self::from_table(alias, &table, context, index)
    }

    pub(crate) fn new(alias: &AliasName, relation: &FullName, column: &ColumnName) -> Self {
        Self {
            alias: alias.as_str().to_owned(),
            relation: relation.clone(),
            column: column.as_str().to_owned(),
        }
    }
    pub(crate) fn alias(&self) -> &str {
        &self.alias
    }
    pub(crate) const fn relation(&self) -> &FullName {
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
    pub(crate) const fn function_count(&self) -> usize {
        self.functions.len()
    }

    pub(crate) fn has_valid_output_layout(&self) -> bool {
        let fields: usize = self
            .functions
            .iter()
            .map(WildcardRowsFunction::column_count)
            .sum();
        self.output_names.len() == fields + usize::from(self.ordinality)
    }

    /// Associate each physical invocation's fields with the final aliased labels.
    /// Callers validate the retained layout before emitting syntax.
    pub(crate) fn try_project_functions<E>(
        &self,
        mut project: impl FnMut(usize, &WildcardRowsProjection<'_>) -> Result<(), E>,
    ) -> Result<(), E> {
        let mut offset = 0;
        for (index, function) in self.functions.iter().enumerate() {
            let end = offset + function.column_count();
            let projection = WildcardRowsProjection::new(function, &self.output_names[offset..end]);
            project(index, &projection)?;
            offset = end;
        }
        Ok(())
    }

    pub(crate) fn ordinality_name(&self) -> Option<&str> {
        if self.ordinality {
            self.output_names.last().map(String::as_str)
        } else {
            None
        }
    }

    /// Include physical and visible spellings when reserving private identifiers.
    pub(crate) fn visit_names(&self, mut visit: impl FnMut(&str)) {
        for function in &self.functions {
            function.visit_names(&mut visit);
        }
        for name in &self.output_names {
            visit(name);
        }
    }

    pub(crate) fn new(
        alias: &AliasName,
        functions: &[WildcardRowsFunction],
        output_names: &[ColumnName],
        ordinality: bool,
    ) -> Self {
        Self {
            alias: alias.as_str().to_owned(),
            functions: functions.to_vec(),
            output_names: output_names.iter().map(ToString::to_string).collect(),
            ordinality,
        }
    }
    pub(crate) fn alias(&self) -> &str {
        &self.alias
    }
}

#[derive(Clone, Debug)]
pub(crate) struct WildcardRowsFunction {
    columns: Vec<String>,
}
impl WildcardRowsFunction {
    const fn column_count(&self) -> usize {
        self.columns.len()
    }

    fn singleton_column_name(&self) -> Option<&str> {
        if let [column] = self.columns.as_slice() {
            Some(column)
        } else {
            None
        }
    }

    fn visit_names(&self, visit: &mut impl FnMut(&str)) {
        for column in &self.columns {
            visit(column);
        }
    }

    pub(crate) fn from_columns(columns: &[ColumnName]) -> Self {
        Self {
            columns: columns.iter().map(ToString::to_string).collect(),
        }
    }
}

/// One invocation's physical fields paired with their final output labels.
pub(crate) struct WildcardRowsProjection<'a> {
    function: &'a WildcardRowsFunction,
    output_names: &'a [String],
}

impl<'a> WildcardRowsProjection<'a> {
    const fn new(function: &'a WildcardRowsFunction, output_names: &'a [String]) -> Self {
        Self {
            function,
            output_names,
        }
    }

    pub(crate) fn singleton_column_name(&self) -> Option<&str> {
        self.function.singleton_column_name()
    }

    pub(crate) fn project_columns(&self, mut project: impl FnMut(&str, &str)) {
        self.function
            .project_columns(self.output_names, &mut project);
    }
}

impl WildcardRowsFunction {
    fn project_columns(&self, output_names: &[String], project: &mut impl FnMut(&str, &str)) {
        for (column, output) in self.columns.iter().zip(output_names) {
            project(column, output);
        }
    }
}
