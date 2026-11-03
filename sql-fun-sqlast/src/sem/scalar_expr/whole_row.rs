use crate::{
    context::ParseContext,
    names::{AliasName, ColumnName, FullName},
    sem::{
        AnalysisError, AnalysisProblem, ColumnCollection, ColumnDefinition, QueryProblem,
        SemScalarExpr, TypeDefinition, TypeReference,
    },
};

/// A whole-row value retains a source occurrence rather than a fabricated column.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WholeRowExpr {
    qualifier: FullName,
    lookup: RowLookup,
    fields: WholeRowFields,
}

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
enum RowLookup {
    Source(RowSource),
    Failed(RowFailure),
}

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
struct RowSource {
    alias: AliasName,
    relation: Option<FullName>,
    type_ref: Option<TypeReference>,
    levels_up: usize,
    not_null: Option<bool>,
}

impl WholeRowExpr {
    pub(super) fn has_renamed_field(&self, rename: &crate::sem::ColumnRename) -> bool {
        self.fields
            .0
            .iter()
            .any(|field| field.value.renamed_column_output(rename).is_some())
    }

    pub(super) fn rename_column(&mut self, rename: &crate::sem::ColumnRename) {
        if let RowLookup::Source(source) = &self.lookup
            && source.relation.as_ref() == Some(rename.table().full_name())
        {
            for field in &mut self.fields.0 {
                if let Some(name) = field.value.renamed_column_output(rename) {
                    field.name = name.clone();
                }
            }
        }
    }

    pub(super) fn rename_relation(
        &mut self,
        old: &crate::names::TableName,
        new: &crate::names::TableName,
    ) {
        if let RowLookup::Source(source) = &mut self.lookup {
            if source.relation.as_ref() == Some(old.full_name()) {
                source.relation = Some(new.full_name().clone());
            }
            if let Some(ty) = &mut source.type_ref {
                ty.rename_relation(old, new);
            }
        }
    }

    pub(crate) fn resolved(
        qualifier: &FullName,
        alias: &AliasName,
        relation: Option<&FullName>,
        type_ref: Option<&TypeReference>,
        levels_up: usize,
        not_null: Option<bool>,
        fields: &WholeRowFields,
    ) -> Self {
        Self {
            qualifier: qualifier.clone(),
            lookup: RowLookup::source(alias, relation, type_ref, levels_up, not_null),
            fields: fields.clone(),
        }
    }

    fn failed(
        context: &mut impl ParseContext,
        qualifier: &FullName,
        failure: RowFailure,
    ) -> Result<SemScalarExpr, AnalysisError> {
        context.report_analysis_problem(AnalysisProblem::query(failure.problem(qualifier)))?;
        Ok(SemScalarExpr::whole_row(&Self {
            qualifier: qualifier.clone(),
            lookup: RowLookup::failed(failure),
            fields: WholeRowFields::default(),
        }))
    }

    /// Requested logical source qualifier.
    pub const fn qualifier(&self) -> &FullName {
        &self.qualifier
    }
    /// Selected local source alias; absent when lookup failed.
    pub fn alias(&self) -> Option<&AliasName> {
        self.lookup.source_ref().map(|s| &s.alias)
    }
    /// Nominal relation identity, when the selected source is a table or view.
    pub const fn relation(&self) -> Option<&FullName> {
        if let Some(source) = self.lookup.source_ref() {
            source.relation.as_ref()
        } else {
            None
        }
    }
    /// Query levels between this reference and its selected source.
    pub fn levels_up(&self) -> Option<usize> {
        self.lookup.source_ref().map(|s| s.levels_up)
    }
    /// Structured completed lookup failure; distinguishes missing and ambiguous sources.
    pub fn failure_code(&self) -> Option<&'static str> {
        self.lookup.failure().map(RowFailure::code)
    }

    pub(super) fn get_type(&self) -> Option<TypeReference> {
        let source = self.lookup.source_ref()?;
        if let Some(ty) = &source.type_ref {
            return Some(ty.clone());
        }
        Some(self.fields.get_type())
    }
    pub(super) const fn is_not_null(&self) -> Option<bool> {
        if let Some(source) = self.lookup.source_ref() {
            source.not_null
        } else {
            None
        }
    }
    pub(super) fn walk<V: crate::sem::traversal::SemanticVisitorMut>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        self.fields.walk(visitor)?;
        Ok(())
    }
    pub(super) fn visit<V: crate::sem::traversal::SemanticVisitor>(
        &self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        self.fields.visit(visitor)?;
        Ok(())
    }
}

impl RowLookup {
    fn source(
        alias: &AliasName,
        relation: Option<&FullName>,
        type_ref: Option<&TypeReference>,
        levels_up: usize,
        not_null: Option<bool>,
    ) -> Self {
        Self::Source(RowSource::new(
            alias, relation, type_ref, levels_up, not_null,
        ))
    }
    const fn failed(failure: RowFailure) -> Self {
        Self::Failed(failure)
    }
    const fn source_ref(&self) -> Option<&RowSource> {
        match self {
            Self::Source(source) => Some(source),
            Self::Failed(_) => None,
        }
    }
    const fn failure(&self) -> Option<&RowFailure> {
        match self {
            Self::Failed(problem) => Some(problem),
            Self::Source(_) => None,
        }
    }
}

impl SemScalarExpr {
    pub(crate) fn whole_row(row: &WholeRowExpr) -> Self {
        Self::WholeRow(Box::new(row.clone()))
    }
    /// Fields exposed by this composite value, including source-occurrence aliases.
    pub(crate) fn composite_columns(
        &self,
        context: &impl ParseContext,
    ) -> Option<ColumnCollection> {
        // A generic record cast preserves a constructor's anonymous field shape.
        if let Some(row) = self.row_constructor() {
            return row.get_type().row_columns(context);
        }
        // A scalar table-function source also retains projection fields. Its
        // scalar type must not become a composite merely because fields exist.
        let columns = self.get_type()?.row_columns(context)?;
        if let Some(row) = self.as_whole_row() {
            Some(row.fields.columns())
        } else {
            Some(columns)
        }
    }

    /// Completed qualified whole-row binding, including failed lookup outcomes.
    pub fn as_whole_row(&self) -> Option<&WholeRowExpr> {
        match self {
            Self::WholeRow(row) => Some(row),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
enum RowFailure {
    NotFound,
    Ambiguous,
    DatabaseRequired,
    CrossDatabase,
}
impl RowFailure {
    const fn not_found() -> Self {
        Self::NotFound
    }
    const fn ambiguous() -> Self {
        Self::Ambiguous
    }
    const fn database_required() -> Self {
        Self::DatabaseRequired
    }
    const fn cross_database() -> Self {
        Self::CrossDatabase
    }
    fn problem(&self, qualifier: &FullName) -> QueryProblem {
        let reference = format!("{qualifier}.*");
        match self {
            Self::NotFound => QueryProblem::column_source_not_found(&reference),
            Self::Ambiguous => QueryProblem::column_source_ambiguous(&reference),
            Self::DatabaseRequired => QueryProblem::column_database_name_required(&reference),
            Self::CrossDatabase => QueryProblem::column_cross_database(&reference),
        }
    }
    const fn code(&self) -> &'static str {
        match self {
            Self::NotFound => "column_source_not_found",
            Self::Ambiguous => "column_source_ambiguous",
            Self::DatabaseRequired => "column_database_name_required",
            Self::CrossDatabase => "column_cross_database",
        }
    }
}
impl WholeRowExpr {
    pub(crate) fn not_found_after_notification(qualifier: &FullName) -> SemScalarExpr {
        SemScalarExpr::whole_row(&Self {
            qualifier: qualifier.clone(),
            lookup: RowLookup::failed(RowFailure::not_found()),
            fields: WholeRowFields::default(),
        })
    }

    pub(crate) fn not_found(
        context: &mut impl ParseContext,
        qualifier: &FullName,
    ) -> Result<SemScalarExpr, AnalysisError> {
        Self::failed(context, qualifier, RowFailure::not_found())
    }
    pub(crate) fn ambiguous(
        context: &mut impl ParseContext,
        qualifier: &FullName,
    ) -> Result<SemScalarExpr, AnalysisError> {
        Self::failed(context, qualifier, RowFailure::ambiguous())
    }
    pub(crate) fn database_required(
        context: &mut impl ParseContext,
        qualifier: &FullName,
    ) -> Result<SemScalarExpr, AnalysisError> {
        Self::failed(context, qualifier, RowFailure::database_required())
    }
    pub(crate) fn cross_database(
        context: &mut impl ParseContext,
        qualifier: &FullName,
    ) -> Result<SemScalarExpr, AnalysisError> {
        Self::failed(context, qualifier, RowFailure::cross_database())
    }
}

#[derive(Debug, Clone, Default, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct WholeRowFields(Vec<WholeRowField>);

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
struct WholeRowField {
    name: ColumnName,
    value: SemScalarExpr,
}
impl WholeRowField {
    fn new(name: &ColumnName, value: &SemScalarExpr) -> Self {
        Self {
            name: name.clone(),
            value: value.clone(),
        }
    }
    fn definition(&self) -> ColumnDefinition {
        ColumnDefinition::new(
            &Some(self.name.clone()),
            self.value.get_type().as_ref(),
            self.value.is_not_null(),
            &[],
        )
    }
}
impl WholeRowFields {
    pub(crate) fn scalar_not_null(&self) -> Option<bool> {
        match self.0.as_slice() {
            [field] => field.value.is_not_null(),
            _ => None,
        }
    }

    pub(crate) fn push(&mut self, name: &ColumnName, value: &SemScalarExpr) {
        self.0.push(WholeRowField::new(name, value));
    }
    fn columns(&self) -> ColumnCollection {
        ColumnCollection::new(self.0.iter().map(WholeRowField::definition).collect())
    }
    fn get_type(&self) -> TypeReference {
        TypeReference::dynamic(&TypeDefinition::implicit_composite(&self.columns()))
    }
    fn walk<V: crate::sem::traversal::SemanticVisitorMut>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        for field in &mut self.0 {
            field.value.walk(visitor)?;
        }
        Ok(())
    }
    fn visit<V: crate::sem::traversal::SemanticVisitor>(
        &self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        for field in &self.0 {
            field.value.visit(visitor)?;
        }
        Ok(())
    }
}

impl RowSource {
    fn new(
        alias: &AliasName,
        relation: Option<&FullName>,
        type_ref: Option<&TypeReference>,
        levels_up: usize,
        not_null: Option<bool>,
    ) -> Self {
        Self {
            alias: alias.clone(),
            relation: relation.cloned(),
            type_ref: type_ref.cloned(),
            levels_up,
            not_null,
        }
    }
}
