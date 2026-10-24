use crate::sem::{
    AliasName, AnalysisError, ColumnName, DataSource, FullName, ParseContext, SemScalarExpr,
    WithClause, data_source::MergeRecordRole,
};

/// A visible source occurrence, selected without a second lookup by bare alias.
#[derive(Debug, Clone)]
pub(crate) struct QualifiedSource<'a> {
    source: &'a DataSource,
    alias: AliasName,
    nullable: bool,
    merge_role: Option<MergeRecordRole>,
    projection_ordinal: Option<usize>,
}

impl<'a> QualifiedSource<'a> {
    pub(super) const fn with_projection_ordinal(mut self, ordinal: usize) -> Self {
        self.projection_ordinal = Some(ordinal);
        self
    }

    pub(super) fn typed_record(
        &self,
        context: &impl ParseContext,
        index: usize,
    ) -> Option<crate::sem::WildcardTypedRecord> {
        if let DataSource::View(source) = self.source {
            let column = context
                .get_view(source.view_name())?
                .result_columns()
                .get_column_at(index)?
                .column_name()
                .as_ref()?;
            return Some(crate::sem::WildcardTypedRecord::new(
                &self.alias,
                source.view_name().full_name(),
                column,
            ));
        }
        let table = match self.source {
            DataSource::Table(table) => table.name().clone(),
            DataSource::CallTableFunc(source) => source
                .function()
                .ret_type()
                .as_ref()?
                .definition_from_catalog(context)?
                .row_table_name(context)?,
            _ => return None,
        };
        let column = context
            .get_table(&table)?
            .get_column_at(index)?
            .name()
            .as_ref()?;
        Some(crate::sem::WildcardTypedRecord::new(
            &self.alias,
            table.full_name(),
            column,
        ))
    }

    pub(super) fn wildcard_qualifier(&self) -> Option<crate::sem::WildcardQualifier> {
        let relation = match self.source {
            DataSource::Table(table) => table.name().full_name(),
            DataSource::View(view) => view.view_name().full_name(),
            _ => return None,
        };
        let mut parts = self.projection_reference(&ColumnName::from(""));
        parts.pop();
        Some(crate::sem::WildcardQualifier::new(
            relation,
            &self.alias,
            &parts,
        ))
    }

    pub(super) fn projection_reference(&self, column: &ColumnName) -> Vec<String> {
        let aliases = self.source.get_aliases();
        let relation = if let Some(alias) = aliases.iter().find(|alias| alias.name() == &self.alias)
        {
            self.source.unaliased_relation(alias)
        } else {
            None
        };
        let mut parts = Vec::new();
        if let Some(relation) = relation {
            if let Some(catalog) = relation.catalog() {
                parts.push(catalog.as_str().to_owned());
            }
            if let Some(schema) = relation.schema() {
                parts.push(schema.as_str().to_owned());
            }
            parts.push(relation.local_name());
        } else {
            parts.push(self.alias.as_str().to_owned());
        }
        parts.push(column.as_str().to_owned());
        parts
    }

    pub(crate) fn is_table_dependent(&self, context: &impl ParseContext) -> bool {
        self.source.projection_is_table_dependent(context)
    }

    pub(super) fn new(source: &'a DataSource, alias: &AliasName) -> Self {
        Self {
            source,
            alias: alias.clone(),
            nullable: false,
            merge_role: None,
            projection_ordinal: None,
        }
    }

    pub(super) const fn with_nullability(mut self, nullable: bool) -> Self {
        self.nullable |= nullable;
        self
    }

    pub(super) const fn with_merge(mut self, role: MergeRecordRole, nullable: bool) -> Self {
        self.nullable |= nullable;
        self.merge_role = Some(role);
        self
    }

    pub(crate) const fn alias(&self) -> &AliasName {
        &self.alias
    }

    pub(crate) fn projection_columns(
        &self,
        context: &impl ParseContext,
        with: &WithClause,
    ) -> Result<Vec<super::ProjectionColumn<'a>>, AnalysisError> {
        Ok(self
            .source
            .projection_names(context, with, Some(&self.alias))?
            .into_iter()
            .map(|column| {
                if let Some(role) = self.merge_role {
                    column.with_merge(role, self.nullable)
                } else {
                    column.with_nullability(self.nullable)
                }
            })
            .collect())
    }

    pub(super) fn column_candidates(
        &self,
        context: &impl ParseContext,
        with: &WithClause,
        column: &ColumnName,
    ) -> Result<super::ColumnCandidates, AnalysisError> {
        self.source
            .column_occurrence_candidates(context, with, column, &self.alias)
    }

    pub(crate) fn bind(
        &self,
        context: &mut impl ParseContext,
        with: &WithClause,
        column: &ColumnName,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let canonical;
        let column = if context.dialect() == crate::ast::Dialect::sqlite() {
            canonical = self.source.canonical_relation_column(context, column);
            canonical.as_ref().unwrap_or(column)
        } else {
            column
        };
        let value = if let Some(index) = self.projection_ordinal
            && let DataSource::Table(table) = self.source
        {
            let definition = if let Some(table) = context.get_table(table.name()) {
                table.get_column_at(index)
            } else {
                None
            };
            SemScalarExpr::column_reference(crate::sem::ColumnReferenceExpr::table_column(
                &self.alias,
                column,
                table.name(),
                definition,
                self.nullable,
            ))
        } else if let Some(index) = self.projection_ordinal
            && let DataSource::View(source) = self.source
        {
            let definition = if let Some(view) = context.get_view(source.view_name()) {
                view.result_columns()
                    .get_column_at(index)
                    .map(crate::sem::ResultColumn::create_column_definition)
            } else {
                None
            };
            SemScalarExpr::column_reference(crate::sem::ColumnReferenceExpr::view_column(
                &self.alias,
                column,
                source.view_name(),
                definition.as_ref(),
                self.nullable,
            ))
        } else if let Some(index) = self.projection_ordinal
            && let DataSource::CallTableFunc(function) = self.source
        {
            crate::sem::ColumnReferenceExpr::function_source_ref(
                context,
                &self.alias,
                column,
                Some(function.function()),
                function.output_columns().column_at(index),
                function.arguments(),
                self.nullable,
            )?
            .0
            .with_function_type_declaration(function.output_type_declaration_at(index))
        } else {
            self.source
                .create_column_ref(context, with, &self.alias, column, self.nullable)?
                .0
        };
        Ok(if let Some(role) = self.merge_role {
            value.merge_record_column(role, self.nullable)
        } else {
            value
        })
    }
}

impl DataSource {
    fn column_occurrence_candidates(
        &self,
        context: &impl ParseContext,
        with: &WithClause,
        column: &ColumnName,
        alias: &AliasName,
    ) -> Result<super::ColumnCandidates, AnalysisError> {
        if let Self::JoinExpr(join) = self {
            if join.column_candidates(column, context, with).is_unknown() {
                return Ok(super::ColumnCandidates::unknown());
            }
            let mut aliases = Vec::new();
            for projected in join.project(context, with, None)? {
                if projected.name() == column {
                    aliases.push(super::AliasSpec::from_local_name(alias.as_str()));
                }
            }
            return Ok(super::ColumnCandidates::from(aliases));
        }
        Ok(self.aliases_for_column_resolved(column, context, with))
    }

    pub(super) fn sqlite_qualified_group_source<'a>(
        &'a self,
        context: &impl ParseContext,
        name: &FullName,
        alias: &'a super::AliasSpec,
    ) -> Option<QualifiedSource<'a>> {
        match self {
            Self::Table(table) => table.qualified_source(self, context, name, Some(alias)),
            Self::View(view) => view.qualified_source(self, context, name, Some(alias)),
            Self::Group(group) => group.sqlite_qualified_group_source(context, name, alias),
            _ => None,
        }
    }

    pub(crate) fn qualified_source<'a>(
        &'a self,
        context: &impl ParseContext,
        name: &FullName,
    ) -> Result<Option<QualifiedSource<'a>>, ()> {
        match self {
            Self::Table(table) => Ok(table.qualified_source(self, context, name, None)),
            Self::View(view) => Ok(view.qualified_source(self, context, name, None)),
            Self::JoinExpr(join) => join.qualified_source(context, name),
            Self::Group(group) => group.qualified_source(context, name),
            Self::MergeRecord(record) => record.qualified_source(context, name),
            _ => Ok(None),
        }
    }
}

pub(super) fn relation_matches(
    context: &impl ParseContext,
    name: &FullName,
    relation: &FullName,
    alias: &super::AliasSpec,
) -> bool {
    if context.dialect() == crate::ast::Dialect::sqlite() {
        name.schema()
            .zip(relation.schema())
            .is_some_and(|(requested, actual)| {
                requested.as_str().eq_ignore_ascii_case(actual.as_str())
            })
            && name
                .name()
                .as_str()
                .eq_ignore_ascii_case(alias.name().as_str())
    } else {
        !alias.is_explicit() && name == relation
    }
}

impl QualifiedSource<'_> {
    pub(crate) fn bind_whole_row(
        &self,
        context: &mut impl ParseContext,
        with: &WithClause,
        requested: &FullName,
        levels_up: usize,
    ) -> Result<SemScalarExpr, AnalysisError> {
        if self.source.whole_row_is_missing() {
            return crate::sem::WholeRowExpr::not_found(context, requested);
        }
        let mut fields = crate::sem::WholeRowFields::default();
        for column in self.projection_columns(context, with)? {
            fields.push(column.name(), &column.bind_direct(context, with)?);
        }
        let type_ref = self.source.whole_row_type(context);
        let not_null = if self.nullable {
            Some(false)
        } else if type_ref
            .as_ref()
            .is_some_and(|ty| ty.row_columns(context).is_none())
            && self.source.whole_row_relation().is_none()
        {
            fields.scalar_not_null()
        } else {
            Some(true)
        };
        Ok(SemScalarExpr::whole_row(
            &crate::sem::WholeRowExpr::resolved(
                requested,
                &self.alias,
                self.source.whole_row_relation(),
                type_ref.as_ref(),
                levels_up,
                not_null,
                &fields,
            ),
        ))
    }
}

impl DataSource {
    const fn whole_row_is_missing(&self) -> bool {
        matches!(self, Self::Unknown(..) | Self::UndefinedTableFunc(_))
    }

    fn whole_row_type(&self, context: &impl ParseContext) -> Option<crate::sem::TypeReference> {
        if let Some(relation) = self.whole_row_relation() {
            return Some(crate::sem::TypeReference::concrete_type_ref(
                relation, false,
            ));
        }
        if let Self::CallTableFunc(function) = self {
            let ty = function.function().ret_type().as_ref()?;
            if !ty.is_dynamic() && *ty != crate::sem::TypeReference::from_master_type_name("record")
            {
                let matches_output = if let Some(columns) = ty.row_columns(context) {
                    columns.matches_type_structure_with(function.output_columns(), PartialEq::eq)
                } else {
                    function.output_columns().get_at(0).is_some()
                        && function.output_columns().get_at(1).is_none()
                };
                if matches_output {
                    return Some(ty.clone());
                }
            }
        }
        None
    }

    const fn whole_row_relation(&self) -> Option<&FullName> {
        match self {
            Self::Table(table) => Some(table.name().full_name()),
            Self::View(view) => Some(view.view_name().full_name()),
            _ => None,
        }
    }
}
