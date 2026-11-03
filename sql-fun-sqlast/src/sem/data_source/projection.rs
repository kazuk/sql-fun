use crate::{
    context::ParseContext,
    names::{AliasName, ColumnName, FullName},
    sem::{
        AliasSpec, AnalysisError, CoalesceExpr, ColumnDefinition, ColumnReferenceExpr, DataSource,
        PgBuiltInType, QualifiedColumnFailure, ResultColumnCollection, ScalarAnalysisContext,
        SemScalarExpr, WildcardExpansion, WildcardQualifiers, WildcardRowsSource,
        WildcardRowsSources, WildcardSourceProjection, WildcardTypedRecord, WithClause,
        data_source::{MergeRecordRole, QualifiedSource, function_columns::OutputColumnNames},
        with_stack,
    },
};

/// An expanded column keeps the selected occurrence until its expression is bound.
#[derive(Debug, Clone)]
pub(crate) struct ProjectionColumn<'a> {
    name: ColumnName,
    column: ColumnName,
    alias: Option<AliasName>,
    binding: ProjectionBinding<'a>,
    origin: Option<ProjectionOrigin<'a>>,
    typed_record: Option<WildcardTypedRecord>,
    rows_origin: Option<(std::sync::Arc<WildcardRowsSource>, usize)>,
    merge_record: Option<(MergeRecordRole, bool)>,
}

#[derive(Clone, Debug)]
struct ProjectionOrigin<'a> {
    source: std::sync::Arc<ProjectionSource<'a>>,
    ordinal: usize,
}

impl<'a> ProjectionOrigin<'a> {
    const fn source(&self) -> &std::sync::Arc<ProjectionSource<'a>> {
        &self.source
    }
    const fn ordinal(&self) -> usize {
        self.ordinal
    }

    fn new(source: &std::sync::Arc<ProjectionSource<'a>>, ordinal: usize) -> Self {
        Self {
            source: std::sync::Arc::clone(source),
            ordinal,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ProjectionSource<'a> {
    alias: AliasName,
    columns: Vec<ProjectionColumn<'a>>,
}

impl<'a> ProjectionSource<'a> {
    const fn alias(&self) -> &AliasName {
        &self.alias
    }
    fn shares_alias(&self, other: &Self) -> bool {
        self.alias == other.alias
    }

    fn new(alias: &AliasName, columns: &[ProjectionColumn<'a>]) -> Self {
        Self {
            alias: alias.clone(),
            columns: columns.to_vec(),
        }
    }
    fn resolve_wildcard(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<WildcardSourceProjection, AnalysisError> {
        with_stack(|| {
            let mut projection = WildcardSourceProjection::new(&self.alias);
            let mut sources = ProjectionSources::default();
            for column in &self.columns {
                let expression = column.bind(context, scope)?;
                projection.append_bound_column(column, &expression);
                column.collect_projection_sources(&mut sources);
            }
            projection.retain_source_projections(&sources.resolve(context, scope)?);
            Ok(projection)
        })
    }
}

#[derive(Default)]
struct ProjectionSources<'a, 'b>(Vec<&'b ProjectionSource<'a>>);
impl<'a, 'b> ProjectionSources<'a, 'b> {
    fn include(&mut self, source: &'b ProjectionSource<'a>) {
        if !self.0.iter().any(|existing| existing.shares_alias(source)) {
            self.0.push(source);
        }
    }
    fn resolve(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<Vec<WildcardSourceProjection>, AnalysisError> {
        let mut projections = Vec::new();
        for source in &self.0 {
            projections.push(source.resolve_wildcard(context, scope)?);
        }
        Ok(projections)
    }
}

#[derive(Debug, Clone)]
enum ProjectionBinding<'a> {
    Source(QualifiedSource<'a>),
    Expression(Box<SemScalarExpr>),
    Merged(Box<MergedProjection<'a>>),
}

#[derive(Debug, Clone)]
struct MergedProjection<'a> {
    left: Box<ProjectionColumn<'a>>,
    right: Box<ProjectionColumn<'a>>,
    nullable: bool,
}

impl<'a> MergedProjection<'a> {
    fn collect_rows_sources(&self, sources: &mut WildcardRowsSources) {
        self.left.collect_rows_sources(sources);
        self.right.collect_rows_sources(sources);
    }
    fn collect_qualifiers(&self, qualifiers: &mut WildcardQualifiers) {
        self.left.collect_qualifiers(qualifiers);
        self.right.collect_qualifiers(qualifiers);
    }

    fn new(left: &ProjectionColumn<'a>, right: &ProjectionColumn<'a>) -> Self {
        Self {
            left: Box::new(left.clone()),
            right: Box::new(right.clone()),
            nullable: false,
        }
    }
    const fn with_nullability(mut self, nullable: bool) -> Self {
        self.nullable |= nullable;
        self
    }

    fn collect_projection_sources<'b>(&'b self, sources: &mut ProjectionSources<'a, 'b>) {
        self.left.collect_projection_sources(sources);
        self.right.collect_projection_sources(sources);
    }

    fn bind(
        &self,
        context: &mut impl ParseContext,
        with: &WithClause,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let left = self.left.bind_direct(context, with)?;
        let right = self.right.bind_direct(context, with)?;
        let not_null = left.is_not_null() == Some(true) && right.is_not_null() == Some(true);
        let left = self
            .left
            .clone()
            .with_nullability(true)
            .bind_direct(context, with)?;
        let right = self
            .right
            .clone()
            .with_nullability(true)
            .bind_direct(context, with)?;
        CoalesceExpr::resolve_join_key(
            context,
            &[left, right],
            self.left.name(),
            not_null && !self.nullable,
        )
    }
}

impl<'a> From<MergedProjection<'a>> for ProjectionBinding<'a> {
    fn from(projection: MergedProjection<'a>) -> Self {
        Self::Merged(Box::new(projection))
    }
}

impl<'a> ProjectionBinding<'a> {
    fn collect_rows_sources(&self, sources: &mut WildcardRowsSources) {
        if let Self::Merged(merged) = self {
            merged.collect_rows_sources(sources);
        }
    }
    fn collect_qualifiers(&self, qualifiers: &mut WildcardQualifiers) {
        match self {
            Self::Source(source) => source.retain_wildcard_qualifier(qualifiers),
            Self::Merged(merged) => merged.collect_qualifiers(qualifiers),
            Self::Expression(_) => {}
        }
    }

    fn merged(left: &ProjectionColumn<'a>, right: &ProjectionColumn<'a>) -> Self {
        Self::Merged(Box::new(MergedProjection::new(left, right)))
    }
    const fn is_merged(&self) -> bool {
        matches!(self, Self::Merged(_))
    }
    fn visible_reference(&self, column: &ColumnName) -> Option<Vec<String>> {
        match self {
            Self::Source(source) => Some(source.projection_reference(column)),
            _ => None,
        }
    }

    fn collect_projection_sources<'b>(&'b self, sources: &mut ProjectionSources<'a, 'b>) {
        if let Self::Merged(merged) = self {
            merged.collect_projection_sources(sources);
        }
    }

    const fn source(source: QualifiedSource<'a>) -> Self {
        Self::Source(source)
    }
    fn expression(expression: SemScalarExpr) -> Self {
        Self::Expression(Box::new(expression))
    }

    fn with_nullability(self, nullable: bool) -> Self {
        match self {
            Self::Source(source) => Self::Source(source.with_nullability(nullable)),
            Self::Expression(expression) => Self::Expression(Box::new(
                expression.with_function_source_nullability(nullable),
            )),
            Self::Merged(merged) => Self::from(merged.with_nullability(nullable)),
        }
    }
    fn bind_direct(
        &self,
        context: &mut impl ParseContext,
        with: &WithClause,
        column: &ColumnName,
    ) -> Result<SemScalarExpr, AnalysisError> {
        match self {
            Self::Source(source) => source.bind(context, with, column),
            Self::Expression(expression) => Ok(expression.as_ref().clone()),
            Self::Merged(merged) => merged.bind(context, with),
        }
    }
    fn bind(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        column: &ColumnName,
    ) -> Result<SemScalarExpr, AnalysisError> {
        match self {
            Self::Source(source) => Ok(scope.bind_source(context, source, column)?.0),
            Self::Expression(expression) => Ok(expression.as_ref().clone()),
            Self::Merged(merged) => merged.bind(context, scope.with_clause()),
        }
    }
}

impl<'a> ProjectionColumn<'a> {
    fn retain_rows_source(&mut self, source: &std::sync::Arc<WildcardRowsSource>, index: usize) {
        self.rows_origin = Some((std::sync::Arc::clone(source), index));
    }

    fn with_occurrence(self, role: Option<MergeRecordRole>, nullable: bool) -> Self {
        if let Some(role) = role {
            self.with_merge(role, nullable)
        } else {
            self.with_nullability(nullable)
        }
    }

    fn requires_join_projection(&self) -> bool {
        match &self.binding {
            ProjectionBinding::Source(source) => source.is_joined(),
            ProjectionBinding::Merged(_) => true,
            ProjectionBinding::Expression(_) => false,
        }
    }
    fn bind_projection(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        binding_qualifier: Option<&AliasName>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        if let Some(qualifier) = binding_qualifier {
            Ok(scope
                .resolve_column(context, Some(qualifier), &self.name)?
                .0)
        } else {
            self.bind(context, scope)
        }
    }
    fn projection_reference(&self, binding_qualifier: Option<&AliasName>) -> Option<Vec<String>> {
        if let Some(alias) = binding_qualifier {
            Some(vec![
                alias.as_str().to_owned(),
                self.name.as_str().to_owned(),
            ])
        } else {
            self.visible_reference()
        }
    }

    pub(crate) fn collect_rows_sources(&self, sources: &mut WildcardRowsSources) {
        with_stack(|| {
            if let Some((source, _)) = &self.rows_origin
                && self
                    .alias
                    .as_ref()
                    .is_none_or(|alias| alias.as_str() == source.alias())
            {
                sources.include(source);
            }
            self.binding.collect_rows_sources(sources);
        });
    }
    pub(crate) fn typed_record(&self) -> Option<WildcardTypedRecord> {
        if self.alias.is_none() {
            self.typed_record.clone()
        } else {
            None
        }
    }

    pub(crate) fn collect_qualifiers(&self, qualifiers: &mut WildcardQualifiers) {
        with_stack(|| {
            self.binding.collect_qualifiers(qualifiers);
        });
    }
    fn with_derived_name(mut self, alias: &AliasSpec, index: usize) -> Self {
        if let Some(name) = alias.derived_column_at(index) {
            self.name = name.clone();
        }
        self
    }
    fn with_source_alias(
        mut self,
        alias: &AliasName,
        source: Option<&std::sync::Arc<ProjectionSource<'a>>>,
        ordinal: usize,
    ) -> Self {
        self.alias = Some(alias.clone());
        if let Some(source) = source {
            self.origin = Some(ProjectionOrigin::new(source, ordinal));
        }
        self
    }
    const fn requires_fixed_children(&self) -> bool {
        self.origin.is_some() || self.rows_origin.is_some()
    }
    pub(crate) fn source_ordinal(&self) -> Option<(&str, usize)> {
        if let Some(origin) = &self.origin
            && self
                .alias
                .as_ref()
                .is_none_or(|alias| alias == origin.source().alias())
        {
            return Some((origin.source().alias().as_str(), origin.ordinal()));
        }
        if let Some((source, ordinal)) = &self.rows_origin
            && self
                .alias
                .as_ref()
                .is_none_or(|alias| alias.as_str() == source.alias())
        {
            return Some((source.alias(), *ordinal));
        }
        None
    }
    fn collect_projection_sources<'b>(&'b self, sources: &mut ProjectionSources<'a, 'b>) {
        with_stack(|| {
            if let Some(origin) = &self.origin {
                sources.include(origin.source());
            }
            self.binding.collect_projection_sources(sources);
        });
    }
    pub(crate) fn visible_reference(&self) -> Option<Vec<String>> {
        if let Some(alias) = &self.alias {
            Some(vec![
                alias.as_str().to_owned(),
                self.name.as_str().to_owned(),
            ])
        } else {
            self.binding.visible_reference(&self.column)
        }
    }
    pub(super) const fn is_merged(&self) -> bool {
        self.binding.is_merged()
    }
    pub(super) fn new(
        source: &QualifiedSource<'a>,
        name: &ColumnName,
        typed_record: &Option<WildcardTypedRecord>,
    ) -> Self {
        Self {
            column: name.clone(),
            name: name.clone(),
            alias: None,
            binding: ProjectionBinding::source(source.clone()),
            origin: None,
            typed_record: typed_record.clone(),
            rows_origin: None,
            merge_record: None,
        }
    }
    pub(super) fn expression(name: &ColumnName, expression: &SemScalarExpr) -> Self {
        Self {
            name: name.clone(),
            column: name.clone(),
            alias: None,
            binding: ProjectionBinding::expression(expression.clone()),
            origin: None,
            typed_record: None,
            rows_origin: None,
            merge_record: None,
        }
    }
    pub(super) fn merged(left: &Self, right: &Self) -> Self {
        Self {
            name: left.name.clone(),
            column: left.name.clone(),
            alias: None,
            origin: None,
            typed_record: None,
            rows_origin: None,
            merge_record: None,
            binding: ProjectionBinding::merged(left, right),
        }
    }
    pub(crate) fn ambiguous(
        context: &mut impl ParseContext,
        qualifier: &AliasName,
    ) -> Result<Self, AnalysisError> {
        let name = ColumnName::from("*");
        let failure = QualifiedColumnFailure::ambiguous(
            context,
            &FullName::from_local_name(qualifier.as_str()),
            &name,
        )?;
        Ok(Self {
            column: name.clone(),
            name,
            alias: None,
            binding: ProjectionBinding::expression(failure),
            origin: None,
            typed_record: None,
            rows_origin: None,
            merge_record: None,
        })
    }
    pub(crate) const fn name(&self) -> &ColumnName {
        &self.name
    }
    pub(crate) fn bind(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let expression = self.binding.bind(context, scope, &self.column)?;
        Ok(self.finish_binding(expression))
    }
    fn finish_binding(&self, expression: SemScalarExpr) -> SemScalarExpr {
        let expression = self.relabel(expression);
        if let Some((role, nullable)) = self.merge_record {
            expression.merge_record_column(role, nullable)
        } else {
            expression
        }
    }
    fn relabel(&self, expression: SemScalarExpr) -> SemScalarExpr {
        if let Some(alias) = &self.alias {
            expression.with_source_reference_name(alias, &self.name)
        } else {
            expression
        }
    }
    pub(super) fn with_join_alias(mut self, alias: &AliasName) -> Self {
        self.alias = Some(alias.clone());
        self
    }
    pub(super) fn bind_direct(
        &self,
        context: &mut impl ParseContext,
        with: &WithClause,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let expression = self.binding.bind_direct(context, with, &self.column)?;
        Ok(self.finish_binding(expression))
    }
    pub(super) fn with_nullability(mut self, nullable: bool) -> Self {
        self.binding = self.binding.with_nullability(nullable);
        self
    }
    pub(super) fn with_merge(mut self, role: MergeRecordRole, nullable: bool) -> Self {
        self.binding = self.binding.with_nullability(nullable);
        self.merge_record = Some((role, nullable));
        self
    }
    pub(super) fn with_group(mut self, source: &'a DataSource, alias: &AliasName) -> Self {
        if self.is_merged() {
            self.alias = Some(alias.clone());
            return self;
        }
        self.binding = ProjectionBinding::source(QualifiedSource::new(source, alias));
        self.alias = None;
        self
    }
}

impl DataSource {
    pub(crate) fn projection_names(
        &self,
        context: &impl ParseContext,
        with: &WithClause,
        qualifier: Option<&AliasName>,
    ) -> Result<Vec<ProjectionColumn<'_>>, AnalysisError> {
        if let Self::RowFunctions(source) = self {
            return source.projection_names(context, with, qualifier);
        }
        if let Self::MergeRecord(record) = self {
            return record.projection_names(context, with, qualifier);
        }
        if let Self::JoinExpr(join) = self {
            return join.project(context, with, qualifier);
        }
        if qualifier.is_none()
            && let Self::Group(group) = self
        {
            return group.projection_names(self, context, with);
        }
        let mut columns = Vec::new();
        for alias in self.get_aliases() {
            if qualifier.is_some_and(|qualifier| qualifier != alias.name()) {
                continue;
            }
            let Some((source, nullable)) = self.resolve_alias(alias.name()) else {
                continue;
            };
            let names = OutputColumnNames::from(source.column_names(context, with)?);
            columns.extend(names.project(source, &alias, nullable, context));
        }
        Ok(columns)
    }
    pub(super) fn column_names(
        &self,
        context: &impl ParseContext,
        with: &WithClause,
    ) -> Result<Vec<ColumnName>, AnalysisError> {
        match self {
            Self::RangeTableSample(sample) => sample.source().column_names(context, with),
            Self::Table(table) => table.projection_column_names(context),
            Self::CteQuery(cte) => cte.projection_column_names(with),
            Self::View(view) => view.projection_column_names(context),
            Self::JoinExpr(join) => join.projection_column_names(context, with),
            Self::Group(group) => group.projection_column_names(self, context, with),
            Self::Unknown(_, _) | Self::UndefinedTableFunc(_) => Ok(Vec::new()),
            Self::CallTableFunc(source) => Ok(source.column_names()),
            Self::DeferredTableFunc(source) => Ok(source.column_names()),
            Self::RowFunctions(source) => source.column_names(context, with),
            Self::MergeRecord(record) => record.column_names(context, with),
            Self::SubQuery(query) => Ok(query.projection_column_names()),
        }
    }
}

/// Projection occurrences retain order and duplicates until column binding.
#[derive(Default)]
pub(super) struct ProjectionColumns<'a>(Vec<ProjectionColumn<'a>>);

impl<'a> From<Vec<ProjectionColumn<'a>>> for ProjectionColumns<'a> {
    fn from(columns: Vec<ProjectionColumn<'a>>) -> Self {
        Self(columns)
    }
}

impl<'a> ProjectionColumns<'a> {
    pub(super) fn with_join_alias(self, alias: &AliasSpec) -> Self {
        let named = self.with_derived_names(alias);
        let source = if named.has_duplicate_names()
            || named
                .0
                .iter()
                .any(ProjectionColumn::requires_fixed_children)
        {
            Some(std::sync::Arc::new(ProjectionSource::new(
                alias.name(),
                &named.0,
            )))
        } else {
            None
        };
        Self(
            named
                .0
                .into_iter()
                .enumerate()
                .map(|(index, column)| {
                    column.with_source_alias(alias.name(), source.as_ref(), index)
                })
                .collect(),
        )
    }
    fn with_derived_names(self, alias: &AliasSpec) -> Self {
        Self(
            self.0
                .into_iter()
                .enumerate()
                .map(|(index, column)| column.with_derived_name(alias, index))
                .collect(),
        )
    }
    fn has_duplicate_names(&self) -> bool {
        self.0.iter().enumerate().any(|(index, column)| {
            self.0[..index]
                .iter()
                .any(|other| other.name() == column.name())
        })
    }

    pub(super) fn with_occurrence(self, role: Option<MergeRecordRole>, nullable: bool) -> Self {
        Self(
            self.0
                .into_iter()
                .map(|column| column.with_occurrence(role, nullable))
                .collect(),
        )
    }

    pub(super) fn with_rows_source(mut self, source: &WildcardRowsSource) -> Self {
        let source = std::sync::Arc::new(source.clone());
        for (index, column) in self.0.iter_mut().enumerate() {
            column.retain_rows_source(&source, index);
        }
        self
    }

    pub(super) fn append_expression(
        &mut self,
        value: &SemScalarExpr,
        name: &str,
        context: &impl ParseContext,
    ) {
        let alias = AliasName::from(name);
        if let Some(fields) = value.composite_columns(context) {
            for (name, member) in fields.project_expression(value) {
                self.0.push(ProjectionColumn::expression(
                    &name,
                    &SemScalarExpr::source_column(&member, &alias, &name),
                ));
            }
        } else {
            let name = ColumnName::from(name);
            self.0.push(ProjectionColumn::expression(
                &name,
                &SemScalarExpr::source_column(value, &alias, &name),
            ));
        }
    }
    pub(super) fn append_source(
        &mut self,
        source: &'a DataSource,
        context: &impl ParseContext,
        with: &WithClause,
    ) -> Result<(), AnalysisError> {
        self.0.extend(source.projection_names(context, with, None)?);
        Ok(())
    }
    pub(super) fn append_ordinality(&mut self, alias: &AliasSpec) {
        let name = ColumnName::from("ordinality");
        let definition = ColumnDefinition::new(
            &Some(name.clone()),
            Some(&PgBuiltInType::int8()),
            Some(true),
            &[],
        );
        let expression = ColumnReferenceExpr::function_output(alias, &definition);
        self.0
            .push(ProjectionColumn::expression(&name, &expression));
    }
    pub(super) fn with_aliases(self, alias: &AliasSpec) -> Self {
        let named = self.with_derived_names(alias);
        Self(
            named
                .0
                .into_iter()
                .map(|column| column.with_join_alias(alias.name()))
                .collect(),
        )
    }
    pub(super) fn names(&self) -> Vec<ColumnName> {
        self.0.iter().map(|column| column.name().clone()).collect()
    }
    pub(super) fn into_columns(self) -> Vec<ProjectionColumn<'a>> {
        self.0
    }
    pub(super) fn bind_column<C: ParseContext>(
        self,
        context: &mut C,
        with: &WithClause,
        alias: &AliasName,
        name: &ColumnName,
        nullable: bool,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let mut matches = self.0.into_iter().filter(|column| column.name() == name);
        let Some(column) = matches.next() else {
            return Ok(ColumnReferenceExpr::function_source_ref(
                context,
                alias,
                name,
                None,
                None,
                &::sql_fun_core::IVec::default(),
                nullable,
            )?
            .0);
        };
        if matches.next().is_some() {
            return ColumnReferenceExpr::ambiguous_column_resolved(context, name);
        }
        column.with_nullability(nullable).bind_direct(context, with)
    }
}

/// One resolved source projection, retaining source identity even at zero width.
pub(crate) struct WildcardProjection<'a> {
    columns: Vec<ProjectionColumn<'a>>,
    source_found: bool,
    table_dependent: bool,
    source_scope_depth: usize,
}

impl<'a> WildcardProjection<'a> {
    pub(crate) const fn unresolved() -> Self {
        Self {
            columns: Vec::new(),
            source_found: false,
            table_dependent: false,
            source_scope_depth: 0,
        }
    }

    pub(crate) fn ambiguous_alias(
        context: &mut impl ParseContext,
        qualifier: &AliasName,
    ) -> Result<Self, AnalysisError> {
        Ok(Self::new(
            &[ProjectionColumn::ambiguous(context, qualifier)?],
            true,
            false,
        ))
    }

    pub(crate) const fn source_found(&self) -> bool {
        self.source_found
    }
    pub(crate) fn append_results(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        results: &mut ResultColumnCollection,
        location: Option<usize>,
        binding_qualifier: Option<&AliasName>,
    ) -> Result<bool, AnalysisError> {
        let first = results.len();
        let mut references = Vec::new();
        let mut sources = ProjectionSources::default();
        for column in &self.columns {
            let expression = column.bind_projection(context, scope, binding_qualifier)?;
            results.push_expanded(column.name(), expression);
            references.push(column.projection_reference(binding_qualifier));
            column.collect_projection_sources(&mut sources);
        }
        if self
            .columns
            .iter()
            .any(ProjectionColumn::requires_join_projection)
        {
            results.mark_join_sensitive_star_since(first);
        }
        let mut expansion =
            WildcardExpansion::new(location, results, first, self.table_dependent, &references);
        expansion.retain_source_scope(self.source_scope_depth);
        expansion.retain_source_projections(&sources.resolve(context, scope)?);
        for column in &self.columns {
            expansion.retain_projection_origin(column);
        }
        context.analysis_state_mut().record_wildcard(expansion);
        Ok(self.source_found)
    }

    pub(crate) fn new(
        columns: &[ProjectionColumn<'a>],
        source_found: bool,
        table_dependent: bool,
    ) -> Self {
        let source_found = !columns.is_empty() || source_found;
        Self {
            columns: columns.to_vec(),
            source_found,
            table_dependent,
            source_scope_depth: 0,
        }
    }
    pub(crate) const fn cross_parent_scope(&mut self, crosses_projection: bool) {
        if crosses_projection {
            self.source_scope_depth += 1;
        }
    }
    pub(crate) fn append_row_fields(
        &self,
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
        fields: &mut Vec<SemScalarExpr>,
    ) -> Result<bool, AnalysisError> {
        if self.source_found {
            for column in &self.columns {
                fields.push(column.bind(context, scope)?);
            }
        }
        Ok(self.source_found)
    }
}
