use crate::{
    ast::Dialect,
    context::ParseContext,
    names::{AliasName, ColumnName, FullName},
    sem::{
        AnalysisError, DataSource, SemScalarExpr, TypeReference, WholeRowExpr, WholeRowFields,
        WildcardQualifier, WildcardQualifiers, WildcardTypedRecord, WithClause,
        data_source::{
            AliasSpec, ColumnCandidates, MergeRecordRole, ProjectionColumn, WildcardProjection,
            projection::ProjectionColumns,
        },
    },
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
    pub(super) fn is_joined(&self) -> bool {
        self.source.is_joined()
    }
    pub(super) const fn with_projection_ordinal(mut self, ordinal: usize) -> Self {
        self.projection_ordinal = Some(ordinal);
        self
    }

    pub(super) fn typed_record(
        &self,
        context: &impl ParseContext,
        index: usize,
    ) -> Option<WildcardTypedRecord> {
        self.source
            .typed_projection_record(&self.alias, context, index)
    }

    pub(super) fn retain_wildcard_qualifier(&self, qualifiers: &mut WildcardQualifiers) {
        if let Some(qualifier) = self.wildcard_qualifier() {
            qualifiers.include(&qualifier);
        }
    }

    pub(super) fn wildcard_qualifier(&self) -> Option<WildcardQualifier> {
        Some(WildcardQualifier::new(
            self.source.wildcard_relation()?,
            &self.alias,
            &self.projection_qualifier(),
        ))
    }

    fn projection_qualifier(&self) -> Vec<String> {
        if let Some(relation) = self.source.unaliased_projection_relation(&self.alias) {
            relation.projection_parts()
        } else {
            vec![self.alias.as_str().to_owned()]
        }
    }

    pub(super) fn projection_reference(&self, column: &ColumnName) -> Vec<String> {
        let mut parts = self.projection_qualifier();
        parts.push(column.as_str().to_owned());
        parts
    }

    pub(crate) fn projection_shape_is_resolved(&self) -> bool {
        self.source.projection_shape_is_resolved()
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

    pub(crate) fn resolved_wildcard_projection(
        &self,
        context: &impl ParseContext,
        with: &WithClause,
    ) -> Result<WildcardProjection<'a>, AnalysisError> {
        Ok(WildcardProjection::new(
            &self.projection_columns(context, with)?,
            self.projection_shape_is_resolved(),
            self.is_table_dependent(context),
        ))
    }

    pub(crate) fn projection_columns(
        &self,
        context: &impl ParseContext,
        with: &WithClause,
    ) -> Result<Vec<ProjectionColumn<'a>>, AnalysisError> {
        let columns = ProjectionColumns::from(self.source.projection_names(
            context,
            with,
            Some(&self.alias),
        )?);
        Ok(columns
            .with_occurrence(self.merge_role, self.nullable)
            .into_columns())
    }

    pub(super) fn column_candidates(
        &self,
        context: &impl ParseContext,
        with: &WithClause,
        column: &ColumnName,
    ) -> Result<ColumnCandidates, AnalysisError> {
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
        let column = if context.dialect() == Dialect::sqlite() {
            canonical = self.source.canonical_relation_column(context, column);
            canonical.as_ref().unwrap_or(column)
        } else {
            column
        };
        let value = self.source.bind_projection_column(
            context,
            with,
            &self.alias,
            column,
            self.projection_ordinal,
            self.nullable,
        )?;
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
    ) -> Result<ColumnCandidates, AnalysisError> {
        if let Self::JoinExpr(join) = self {
            if join.column_candidates(column, context, with).is_unknown() {
                return Ok(ColumnCandidates::unknown());
            }
            let mut aliases = Vec::new();
            for projected in join.project(context, with, None)? {
                if projected.name() == column {
                    aliases.push(AliasSpec::from_local_name(alias.as_str()));
                }
            }
            return Ok(ColumnCandidates::from(aliases));
        }
        Ok(self.aliases_for_column_resolved(column, context, with))
    }

    pub(super) fn sqlite_qualified_group_source<'a>(
        &'a self,
        context: &impl ParseContext,
        name: &FullName,
        alias: &'a AliasSpec,
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
            Self::RangeTableSample(sample) => sample.source().qualified_source(context, name),
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
    alias: &AliasSpec,
) -> bool {
    if context.dialect() == Dialect::sqlite() {
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
            return WholeRowExpr::not_found(context, requested);
        }
        let mut fields = WholeRowFields::default();
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
        Ok(SemScalarExpr::whole_row(&WholeRowExpr::resolved(
            requested,
            &self.alias,
            self.source.whole_row_relation(),
            type_ref.as_ref(),
            levels_up,
            not_null,
            &fields,
        )))
    }
}

impl DataSource {
    const fn whole_row_is_missing(&self) -> bool {
        matches!(self, Self::Unknown(..) | Self::UndefinedTableFunc(_))
    }

    fn whole_row_type(&self, context: &impl ParseContext) -> Option<TypeReference> {
        if let Some(relation) = self.whole_row_relation() {
            return Some(TypeReference::concrete_type_ref(relation, false));
        }
        if let Self::CallTableFunc(function) = self {
            let ty = function.function().ret_type().as_ref()?;
            if !ty.is_dynamic() && *ty != TypeReference::from_master_type_name("record") {
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
            Self::RangeTableSample(sample) => sample.source().whole_row_relation(),
            Self::Table(table) => Some(table.name().full_name()),
            Self::View(view) => Some(view.view_name().full_name()),
            _ => None,
        }
    }
}

impl FullName {
    fn projection_parts(&self) -> Vec<String> {
        let mut parts = Vec::new();
        if let Some(catalog) = self.catalog() {
            parts.push(catalog.as_str().to_owned());
        }
        if let Some(schema) = self.schema() {
            parts.push(schema.as_str().to_owned());
        }
        parts.push(self.local_name());
        parts
    }
}

impl DataSource {
    fn typed_projection_record(
        &self,
        alias: &AliasName,
        context: &impl ParseContext,
        index: usize,
    ) -> Option<WildcardTypedRecord> {
        match self {
            Self::Table(source) => source.typed_projection_record(alias, context, index),
            Self::View(source) => source.typed_projection_record(alias, context, index),
            Self::CallTableFunc(source) => source.typed_projection_record(alias, context, index),
            _ => None,
        }
    }

    const fn wildcard_relation(&self) -> Option<&FullName> {
        match self {
            Self::Table(source) => Some(source.wildcard_relation()),
            Self::View(source) => Some(source.wildcard_relation()),
            _ => None,
        }
    }

    fn unaliased_projection_relation(&self, alias: &AliasName) -> Option<&FullName> {
        match self {
            Self::Table(source) => source.unaliased_projection_relation(alias),
            Self::View(source) => source.unaliased_projection_relation(alias),
            _ => None,
        }
    }

    fn bind_named_projection_column(
        &self,
        context: &mut impl ParseContext,
        with: &WithClause,
        alias: &AliasName,
        column: &ColumnName,
        nullable: bool,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let (expression, _) = self.create_column_ref(context, with, alias, column, nullable)?;
        Ok(expression)
    }

    fn bind_projection_column(
        &self,
        context: &mut impl ParseContext,
        with: &WithClause,
        alias: &AliasName,
        column: &ColumnName,
        ordinal: Option<usize>,
        nullable: bool,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let Some(ordinal) = ordinal else {
            return self.bind_named_projection_column(context, with, alias, column, nullable);
        };
        match self {
            Self::Table(source) => {
                Ok(source.bind_projection_ordinal(context, alias, column, ordinal, nullable))
            }
            Self::View(source) => {
                Ok(source.bind_projection_ordinal(context, alias, column, ordinal, nullable))
            }
            Self::CallTableFunc(source) => {
                source.bind_projection_ordinal(context, alias, column, ordinal, nullable)
            }
            _ => self.bind_named_projection_column(context, with, alias, column, nullable),
        }
    }
}
