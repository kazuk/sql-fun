use crate::sem::{
    AliasSpec, AnalysisError, AnalysisProblem, CatalogNameLookup, ColumnCollection,
    ColumnDefinition, ColumnName, FullName, OverloadVariant, ParseContext, TableFunctionProblem,
    TypeReference,
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
    requested_type: crate::sem::WithTypmod<TypeReference>,
    resolved_type: Option<TypeReference>,
    collation: Option<CatalogNameLookup>,
}

impl TableFunctionOutput {
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
        let mut declaration = crate::sem::WithTypmod::new(requested_type, modifiers);
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
        if let Some(collation) = &collation
            && collation.resolved_name().is_none()
        {
            self.report(
                context,
                TableFunctionProblem::collation_not_found(&collation.requested().to_string()),
            )?;
        }
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
        let declared = overload.returning_table();
        let record = overload.requires_record_definition();
        self.validate_requirement(context, record)?;
        if !record {
            self.columns = declared;
            if self.columns.get_at(0).is_none()
                && let Some(ty) = overload.ret_type()
            {
                if let Some(definition) = ty.definition_from_catalog(context)
                    && let Some(columns) = definition.table_function_columns(context)
                {
                    self.columns = columns;
                } else {
                    self.columns.push(&ColumnDefinition::new(
                        &Some(ColumnName::from(alias.name().as_str())),
                        Some(ty),
                        None,
                        &[],
                    ));
                }
            }
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
    pub(crate) fn declaration(
        &self,
        column: &ColumnName,
        alias: &AliasSpec,
    ) -> Option<&crate::sem::WithTypmod<TypeReference>> {
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
            Some(&crate::sem::PgBuiltInType::int8()),
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
    pub(super) fn append_function_columns(
        &mut self,
        source: &crate::sem::DataSource,
    ) -> Option<()> {
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
