use crate::{
    context::ParseContext,
    names::{ColumnName, FullName},
    sem::{
        AliasSpec, AnalysisError, AnalysisProblem, CatalogNameLookup, ColumnCollection,
        ColumnDefinition, DataSource, DeclarationCollation, OverloadVariant, PgBuiltInType,
        TableFunctionProblem, TypeReference, WithTypmod,
        data_source::{ProjectionColumn, QualifiedSource},
    },
};

/// Output owned by one table-function invocation, independently of its signature.
#[derive(Debug, Clone, Default, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct TableFunctionOutput {
    columns: ColumnCollection,
    declarations: Vec<RecordColumn>,
    problems: Vec<TableFunctionProblem>,
}

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
struct RecordColumn {
    name: ColumnName,
    requested_type: WithTypmod<TypeReference>,
    resolved_type: Option<TypeReference>,
    collation: Option<CatalogNameLookup>,
    validation: DeclarationCollation,
}

impl TableFunctionOutput {
    pub(super) fn replace_definition(&mut self, output: &Self) {
        let invalid = self
            .problems
            .contains(&TableFunctionProblem::invalid_expression());
        *self = output.clone();
        if invalid {
            self.retain_invalid_expression();
        }
    }
    pub(super) fn retain_invalid_expression(&mut self) {
        self.problems
            .push(TableFunctionProblem::invalid_expression());
    }
    pub(super) fn inferred(columns: &ColumnCollection) -> Self {
        Self {
            columns: columns.clone(),
            ..Self::default()
        }
    }
    pub(super) fn refresh_inferred(&mut self, columns: &ColumnCollection) {
        if !self.has_declarations() {
            self.columns = columns.clone();
        }
    }
    pub(crate) fn declared(overload: &OverloadVariant) -> Self {
        Self {
            columns: overload.returning_table(),
            ..Self::default()
        }
    }
    pub(crate) fn declare(
        &mut self,
        context: &mut impl ParseContext,
        name: &str,
        requested_type: &TypeReference,
        modifiers: &[String],
        collation: Option<&FullName>,
    ) -> Result<(), AnalysisError> {
        let name = ColumnName::from(name);
        if self.columns.has_column(&name) {
            self.report(
                context,
                TableFunctionProblem::duplicate_column(name.as_str()),
            )?;
        }
        let resolved_type = context.canonicalize_type_ref(requested_type);
        let mut declaration = WithTypmod::new(requested_type, modifiers);
        if let Some(resolved) = &resolved_type {
            declaration.interpret_postgres(resolved);
            if let Some(message) = declaration.validation_error() {
                self.report(context, TableFunctionProblem::invalid_typmod(message))?;
            }
        }
        if resolved_type.is_none() {
            self.report(
                context,
                TableFunctionProblem::type_not_found(&requested_type.to_string()),
            )?;
        }
        let collation = collation.map(|name| context.lookup_collation(name));
        let validation =
            DeclarationCollation::validate(context, resolved_type.as_ref(), collation.as_ref());
        validation.report(context, resolved_type.as_ref())?;
        self.columns.push(&ColumnDefinition::new(
            &Some(name.clone()),
            resolved_type.as_ref(),
            None,
            &[],
        ));
        self.declarations.push(RecordColumn {
            name,
            requested_type: declaration,
            resolved_type,
            collation,
            validation,
        });
        Ok(())
    }

    fn report(
        &mut self,
        context: &mut impl ParseContext,
        problem: TableFunctionProblem,
    ) -> Result<(), AnalysisError> {
        self.problems.push(problem.clone());
        context.report_analysis_problem(AnalysisProblem::table_function(problem))
    }

    pub(crate) fn apply(
        &mut self,
        context: &mut impl ParseContext,
        overload: &OverloadVariant,
        alias: &AliasSpec,
    ) -> Result<(), AnalysisError> {
        let record = overload.requires_record_definition();
        self.validate_requirement(context, record)?;
        if !record {
            self.columns = overload
                .table_function_output_columns(context, &ColumnName::from(alias.name().as_str()));
        }
        Ok(())
    }

    pub(crate) fn validate_requirement(
        &mut self,
        context: &mut impl ParseContext,
        required: bool,
    ) -> Result<(), AnalysisError> {
        if required && self.declarations.is_empty() {
            self.report(context, TableFunctionProblem::definition_required())?;
        } else if !required && !self.declarations.is_empty() {
            self.report(context, TableFunctionProblem::definition_not_allowed())?;
        }
        Ok(())
    }
    pub(super) fn declaration_at(&self, index: usize) -> Option<&WithTypmod<TypeReference>> {
        self.declarations
            .get(index)
            .map(|column| &column.requested_type)
    }

    pub(crate) fn declaration(
        &self,
        column: &ColumnName,
        alias: &AliasSpec,
    ) -> Option<&WithTypmod<TypeReference>> {
        let index = OutputColumnNames::from(self.names()).matching_index(column, alias)?;
        self.declarations
            .get(index)
            .map(|column| &column.requested_type)
    }

    pub(crate) const fn columns(&self) -> &ColumnCollection {
        &self.columns
    }
    pub(crate) fn problems(&self) -> &[TableFunctionProblem] {
        &self.problems
    }
    pub(crate) fn reject_placement(
        &mut self,
        context: &mut impl ParseContext,
    ) -> Result<(), AnalysisError> {
        self.report(context, TableFunctionProblem::invalid_placement())
    }
    pub(crate) fn append_ordinality(&mut self) {
        self.columns.push(&ColumnDefinition::new(
            &Some(ColumnName::from("ordinality")),
            Some(&PgBuiltInType::int8()),
            Some(true),
            &[],
        ));
    }
    pub(crate) fn validate_aliases(
        &mut self,
        context: &mut impl ParseContext,
        alias: &AliasSpec,
    ) -> Result<(), AnalysisError> {
        self.validate_alias_count(context, alias.len_derived_column(), self.names().len())
    }
    pub(crate) fn validate_alias_count(
        &mut self,
        context: &mut impl ParseContext,
        aliases: usize,
        columns: usize,
    ) -> Result<(), AnalysisError> {
        if aliases > columns {
            self.report(context, TableFunctionProblem::too_many_aliases())?;
        }
        Ok(())
    }

    pub(crate) const fn has_declarations(&self) -> bool {
        !self.declarations.is_empty()
    }
    pub(crate) fn names(&self) -> Vec<ColumnName> {
        self.columns.names()
    }

    pub(crate) fn candidates(&self, column: &ColumnName, alias: &AliasSpec) -> Vec<AliasSpec> {
        OutputColumnNames::from(self.names()).candidates(column, alias)
    }

    pub(crate) fn column(
        &self,
        column: &ColumnName,
        alias: &AliasSpec,
    ) -> Option<&ColumnDefinition> {
        let index = OutputColumnNames::from(self.names()).matching_index(column, alias)?;
        self.columns.get_at(index)
    }
}

/// Ordered invocation output names, including duplicates used for ambiguity checks.
#[derive(Default)]
pub(super) struct OutputColumnNames(Vec<ColumnName>);

impl From<Vec<ColumnName>> for OutputColumnNames {
    fn from(names: Vec<ColumnName>) -> Self {
        Self(names)
    }
}

impl OutputColumnNames {
    pub(super) fn project<'a>(
        self,
        source: &'a DataSource,
        alias: &AliasSpec,
        nullable: bool,
        context: &impl ParseContext,
    ) -> Vec<ProjectionColumn<'a>> {
        let names = self.with_derived_names(alias);
        let duplicates = names.has_duplicates();
        let mut columns = Vec::new();
        for (index, name) in names.0.into_iter().enumerate() {
            let source = QualifiedSource::new(source, alias.name())
                .with_nullability(nullable)
                .with_projection_ordinal(index);
            let record = if duplicates {
                source.typed_record(context, index)
            } else {
                None
            };
            columns.push(ProjectionColumn::new(&source, &name, &record));
        }
        columns
    }
    fn with_derived_names(self, alias: &AliasSpec) -> Self {
        Self(
            self.0
                .into_iter()
                .enumerate()
                .map(|(index, name)| alias.projection_name(index, &name))
                .collect(),
        )
    }
    fn has_duplicates(&self) -> bool {
        self.0
            .iter()
            .enumerate()
            .any(|(index, name)| self.0[..index].contains(name))
    }

    pub(super) fn append_names(&mut self, names: &[ColumnName]) {
        self.0.extend_from_slice(names);
    }
    pub(super) fn append_function_columns(&mut self, source: &DataSource) -> Option<()> {
        self.0.extend(source.known_function_columns()?);
        Some(())
    }
    pub(super) fn append_ordinality(&mut self) {
        self.0.push(ColumnName::from("ordinality"));
    }
    pub(super) const fn len(&self) -> usize {
        self.0.len()
    }
    fn matching_index(&self, column: &ColumnName, alias: &AliasSpec) -> Option<usize> {
        self.0
            .iter()
            .enumerate()
            .position(|(index, name)| alias.derived_column_at(index).unwrap_or(name) == column)
    }
    pub(super) fn candidates(&self, column: &ColumnName, alias: &AliasSpec) -> Vec<AliasSpec> {
        let count = self
            .0
            .iter()
            .enumerate()
            .filter(|(index, name)| alias.derived_column_at(*index).unwrap_or(name) == column)
            .count();
        vec![alias.clone(); count]
    }
}
