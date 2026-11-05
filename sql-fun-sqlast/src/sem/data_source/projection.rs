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

#[cfg(test)]
mod test_full_join_using_keeps_merged_key_and_order {

    use crate::{
        names::FullName, sem::PgBuiltInType, test_helpers::query::source_namespace_statement,
    };

    #[rstest::rstest]
    #[case::star("SELECT * FROM a.t FULL JOIN b.t USING(id)")]
    #[case::explicit("SELECT id, only_a, only_b FROM a.t FULL JOIN b.t USING(id)")]
    #[case::aliased("SELECT j.* FROM (a.t FULL JOIN b.t USING(id)) AS j")]
    fn full_join_using_keeps_merged_key_and_order(#[case] sql: &str) {
        let analyzed = source_namespace_statement(sql);
        assert!(
            analyzed.context().reported_problems().is_empty(),
            "{:?}",
            analyzed.context().reported_problems()
        );
        let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
        let columns = query.result_columns();
        assert_eq!(
            columns
                .names()
                .iter()
                .map(|n| n.as_str())
                .collect::<Vec<_>>(),
            ["id", "only_a", "only_b"]
        );
        let merged = columns.get_column_at(0).unwrap();
        assert_eq!(merged.col_type(), &Some(PgBuiltInType::int8()));
        assert_eq!(merged.is_not_null(), &Some(true));
        let coalesce = merged.expression().unwrap().as_coalesce().unwrap();
        assert_eq!(coalesce.arguments().len(), 2);
        assert_eq!(
            coalesce.arguments()[0].get_type(),
            Some(PgBuiltInType::int8())
        );
        assert_eq!(
            coalesce.arguments()[1]
                .table_column_reference()
                .unwrap()
                .relation(),
            &FullName::with_schema("b", "t")
        );
    }

    #[rstest::rstest]
    #[case::nested("SELECT * FROM a.t FULL JOIN b.t USING(id) FULL JOIN c.t USING(id)", &["id", "only_a", "only_b"])]
    #[case::natural("SELECT * FROM a.t NATURAL FULL JOIN b.t", &["id", "only_a", "only_b"])]
    fn full_join_merged_key_survives_other_join_shapes(#[case] sql: &str, #[case] names: &[&str]) {
        let analyzed = source_namespace_statement(sql);
        assert!(
            analyzed.context().reported_problems().is_empty(),
            "{:?}",
            analyzed.context().reported_problems()
        );
        let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
        let columns = query.result_columns();
        assert_eq!(
            columns
                .names()
                .iter()
                .map(|name| name.as_str())
                .collect::<Vec<_>>(),
            names
        );
        let key = columns.get_column_at(0).unwrap();
        assert_eq!(key.col_type(), &Some(PgBuiltInType::int8()));
        assert_eq!(key.is_not_null(), &Some(true));
        assert!(key.expression().unwrap().as_coalesce().is_some());
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
}

#[cfg(test)]
mod test_expands_qualified_join_star {
    use crate::{ast::Dialect, sem::PgBuiltInType, test_helpers::insert::analyze};

    #[rstest::rstest]
    #[case::on(
        "INSERT INTO items SELECT j.* FROM ((SELECT 1 id) a JOIN (SELECT 'one'::text label) b ON true) AS j RETURNING *",
        2
    )]
    #[case::using(
        "INSERT INTO items(id) SELECT j.* FROM ((SELECT 1 id) a JOIN (SELECT 1 id) b USING(id)) AS j RETURNING id",
        1
    )]
    #[case::natural(
        "INSERT INTO items(id) SELECT j.* FROM ((SELECT 1 id) a NATURAL JOIN (SELECT 1 id) b) AS j RETURNING id",
        1
    )]
    #[case::left(
        "INSERT INTO items SELECT j.* FROM ((SELECT 1 id) a LEFT JOIN (SELECT 'one'::text label) b ON false) AS j RETURNING *",
        2
    )]
    fn expands_qualified_join_star(#[case] sql: &str, #[case] width: usize) {
        let (statement, context) = analyze(sql, Dialect::postgres());
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:?}",
            context.reported_problems()
        );
        let insert = statement.as_insert().unwrap();
        assert_eq!(insert.columns().len(), width);
        assert_eq!(insert.result_columns().len(), width);
        assert_eq!(
            insert.result_columns().get_column_at(0).unwrap().col_type(),
            &Some(PgBuiltInType::int4())
        );
    }
}

#[cfg(test)]
mod test_star_preserves_occurrences_and_output_order {

    use crate::{
        names::FullName,
        sem::PgBuiltInType,
        test_helpers::{query::source_namespace_statement, query_source_namespace as test_helpers},
    };

    #[rstest::rstest]
    #[case::comma("SELECT * FROM a.t, b.t", &["a", "a", "b", "b"])]
    #[case::reversed("SELECT * FROM b.t, a.t", &["b", "b", "a", "a"])]
    #[case::schema_qualified("SELECT a.t.* FROM a.t, b.t", &["a", "a"])]
    #[case::join("SELECT * FROM a.t CROSS JOIN b.t", &["a", "a", "b", "b"])]
    #[case::left("SELECT * FROM a.t LEFT JOIN b.t ON true", &["a", "a", "b", "b"])]
    #[case::right("SELECT * FROM a.t RIGHT JOIN b.t ON true", &["a", "a", "b", "b"])]
    #[case::update("UPDATE a.t SET id=1 FROM b.t RETURNING *", &["a", "a"])]
    #[case::delete("DELETE FROM a.t USING b.t RETURNING *", &["a", "a"])]
    fn star_preserves_occurrences_and_output_order(#[case] sql: &str, #[case] schemas: &[&str]) {
        let analyzed = source_namespace_statement(sql);
        assert!(
            analyzed.context().reported_problems().is_empty(),
            "{:?}",
            analyzed.context().reported_problems()
        );
        let columns = test_helpers::result_columns(analyzed.ast());
        assert_eq!(columns.len(), schemas.len());
        for (index, schema) in schemas.iter().enumerate() {
            let column = columns.get_column_at(index).unwrap();
            assert_eq!(
                column
                    .expression()
                    .unwrap()
                    .table_column_reference()
                    .unwrap()
                    .relation(),
                &FullName::with_schema(schema, "t")
            );
            assert_eq!(
                column.col_type(),
                &Some(if *schema == "a" {
                    PgBuiltInType::int4()
                } else {
                    PgBuiltInType::int8()
                })
            );
            let null_extended = (sql.contains("LEFT JOIN") && *schema == "b")
                || (sql.contains("RIGHT JOIN") && *schema == "a");
            assert_eq!(column.is_not_null(), &Some(!null_extended));
        }
    }

    #[rstest::rstest]
    #[case::inner("SELECT * FROM a.t JOIN b.t USING(id)", &["a", "a", "b"])]
    #[case::right("SELECT * FROM a.t RIGHT JOIN b.t USING(id)", &["b", "a", "b"])]
    #[case::natural("SELECT * FROM a.t NATURAL JOIN b.t", &["a", "a", "b"])]
    #[case::aliased("SELECT j.* FROM (a.t CROSS JOIN b.t) j", &["a", "a", "b", "b"])]
    fn joined_star_keeps_selected_sources(#[case] sql: &str, #[case] schemas: &[&str]) {
        let analyzed = source_namespace_statement(sql);
        assert!(
            analyzed.context().reported_problems().is_empty(),
            "{:?}",
            analyzed.context().reported_problems()
        );
        let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
        let columns = query.result_columns();
        assert_eq!(columns.len(), schemas.len());
        for (index, schema) in schemas.iter().enumerate() {
            assert_eq!(
                columns
                    .get_column_at(index)
                    .unwrap()
                    .expression()
                    .unwrap()
                    .table_column_reference()
                    .unwrap()
                    .relation(),
                &FullName::with_schema(schema, "t")
            );
        }
    }
}

#[cfg(test)]
mod test_qualified_star_expands_only_its_selected_source {
    use crate::{
        sem::PgBuiltInType,
        test_helpers::query::{output_type, table_function_query},
    };

    #[rstest::rstest]
    #[case::comma("SELECT s.* FROM (SELECT 1 AS id) s, ROWS FROM(record_fun('') AS(a int)) r")]
    #[case::join(
        "SELECT s.* FROM (SELECT 1 AS id) s CROSS JOIN ROWS FROM(record_fun('') AS(a int)) r"
    )]
    fn qualified_star_expands_only_its_selected_source(#[case] sql: &str) {
        let (query, context) = table_function_query(sql, false);
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:?}",
            context.reported_problems()
        );
        assert_eq!(output_type(&query, "id"), Some(PgBuiltInType::int4()));
        assert!(query.result_columns().get_column_at(1).is_none());
    }
}

#[cfg(test)]
mod test_wildcard_projection_metadata {
    #[cfg(feature = "sqlite")]
    use crate::{ast::Dialect, test_helpers::query::resolve};

    use crate::test_helpers::wildcard::{
        RetainedColumnsAssertions as _, WildcardGroupsAssertions as _,
    };

    use crate::{
        context::ParseContext,
        test_helpers::query::{source_namespace_statement, table_function_query},
    };

    #[rstest::rstest]
    #[case::zero_width("SELECT * FROM a.zero_columns", &[7], &[true], &[""])]
    #[case::qualified_zero_width("SELECT z.* FROM a.zero_columns z", &[7], &[true], &[""])]
    #[case::relation_zero_width("SELECT a.zero_columns.* FROM a.zero_columns", &[7], &[true], &[""])]
    #[case::unqualified("SELECT * FROM a.t", &[7], &[true], &["id,only_a"])]
    #[case::qualified("SELECT t.* FROM a.t", &[7], &[true], &["id,only_a"])]
    #[case::repeated("SELECT t.*, t.* FROM a.t", &[7, 12], &[true, true], &["id,only_a", "id,only_a"])]
    #[case::view("SELECT * FROM a.v", &[7], &[false], &["id"])]
    #[case::fixed_derived("SELECT * FROM (SELECT id FROM a.t) s", &[7], &[false], &["id"])]
    #[case::nested("SELECT * FROM (SELECT * FROM a.t) s", &[22, 7], &[true, false], &["id,only_a", "id,only_a"])]
    #[case::cte("WITH q AS (SELECT * FROM a.t) SELECT * FROM q", &[18, 37], &[true, false], &["id,only_a", "id,only_a"])]
    #[case::using("SELECT * FROM a.t JOIN b.t USING (id)", &[7], &[true], &["id,only_a,only_b"])]
    #[case::natural("SELECT * FROM a.t NATURAL JOIN b.t", &[7], &[true], &["id,only_a,only_b"])]
    #[case::full("SELECT * FROM a.t FULL JOIN b.t USING (id)", &[7], &[true], &["id,only_a,only_b"])]
    fn retains_groups_names_order_and_origins(
        #[case] sql: &str,
        #[case] locations: &[usize],
        #[case] dependencies: &[bool],
        #[case] names: &[&str],
    ) {
        let analyzed = source_namespace_statement(sql);
        assert!(
            analyzed.context().reported_problems().is_empty(),
            "{:?}",
            analyzed.context().reported_problems()
        );
        let groups = analyzed.context().analysis_state().wildcard_expansions();
        assert_eq!(groups.len(), locations.len());
        groups.assert_groups(locations, dependencies, names);
    }

    #[rstest::rstest]
    #[case::insert("INSERT INTO a.t(id, only_a) VALUES (1, 2) RETURNING *")]
    #[case::update("UPDATE a.t SET only_a = 2 RETURNING *")]
    #[case::delete("DELETE FROM a.t RETURNING *")]
    fn returning_records_the_destination_expansion(#[case] sql: &str) {
        let analyzed = source_namespace_statement(sql);
        assert!(
            analyzed.context().reported_problems().is_empty(),
            "{:?}",
            analyzed.context().reported_problems()
        );
        let groups = analyzed.context().analysis_state().wildcard_expansions();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].location(), sql.find('*'));
        assert!(groups[0].is_table_dependent());
        assert_eq!(groups[0].columns().len(), 2);
    }

    #[test]
    fn aliased_merged_projection_retains_visible_reference() {
        let analyzed =
            source_namespace_statement("SELECT j.* FROM (a.t FULL JOIN b.t USING (id)) j");
        let groups = analyzed.context().analysis_state().wildcard_expansions();
        assert_eq!(groups.len(), 1);
        assert_eq!(
            groups[0].reference_at(0),
            Some(["j".to_owned(), "id".to_owned()].as_slice())
        );
    }

    #[cfg(feature = "sqlite")]
    #[test]
    fn sqlite_groups_do_not_require_lexical_locations() {
        let (_, context) = resolve(
            "SELECT s.* FROM (SELECT 1 AS id) s",
            Dialect::sqlite(),
            true,
        )
        .unwrap();
        let groups = context.analysis_state().wildcard_expansions();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].location(), None);
        assert!(!groups[0].is_table_dependent());
        assert_eq!(
            groups[0].columns()[0]
                .column_name()
                .as_ref()
                .unwrap()
                .as_str(),
            "id"
        );
    }

    #[rstest::rstest]
    #[case::fixed_record("SELECT r.* FROM out_fun() r", false)]
    #[case::named_composite("SELECT r.* FROM pair_fun() r", false)]
    #[case::table_row("SELECT r.* FROM row_fun() r", true)]
    #[case::rows_from("SELECT r.* FROM ROWS FROM (row_fun(), pair_fun()) r", true)]
    fn classifies_function_output_by_its_resolved_shape(
        #[case] sql: &str,
        #[case] table_dependent: bool,
    ) {
        let (_, context) = table_function_query(sql, false);
        assert!(
            context.reported_problems().is_empty(),
            "{:?}",
            context.reported_problems()
        );
        let groups = context.analysis_state().wildcard_expansions();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].is_table_dependent(), table_dependent);
    }

    #[rstest::rstest]
    #[case::explicit("SELECT r.* FROM ROWS FROM (row_fun(), pair_fun()) WITH ORDINALITY r(x,x,y,n)", "r", &["x", "x", "y", "n"], Some("n"), "0:a=x;1:a=x;1:b=y;")]
    #[case::implicit("SELECT row_fun.* FROM ROWS FROM (row_fun(), pair_fun())", "row_fun", &["a", "a", "b"], None, "0:a=a;1:a=a;1:b=b;")]
    fn rows_sources_retain_each_function_layout_before_aliasing(
        #[case] sql: &str,
        #[case] alias: &str,
        #[case] output_names: &[&str],
        #[case] ordinality: Option<&str>,
        #[case] expected_layout: &str,
    ) {
        let (_, context) = table_function_query(sql, false);
        assert!(
            context.reported_problems().is_empty(),
            "{:?}",
            context.reported_problems()
        );
        let group = &context.analysis_state().wildcard_expansions()[0];
        let [source] = group.rows_sources() else {
            panic!("one resolved ROWS FROM source")
        };
        assert_eq!(source.alias(), alias);
        assert_eq!(source.function_count(), 2);
        assert!(source.has_valid_output_layout());
        let mut retained = String::new();
        source
            .try_project_functions(|index, projection| {
                projection.project_columns(|physical, visible| {
                    retained.push_str(&format!("{index}:{physical}={visible};"))
                });
                Ok::<(), std::convert::Infallible>(())
            })
            .unwrap();
        assert_eq!(retained, expected_layout);
        assert_eq!(source.ordinality_name(), ordinality);
        for index in 0..output_names.len() {
            assert_eq!(group.source_ordinal_at(index), Some((alias, index)));
        }
    }

    #[rstest::rstest]
    #[case::insert("INSERT INTO a.zero_columns DEFAULT VALUES RETURNING *, 1 AS marker")]
    #[case::delete("DELETE FROM a.zero_columns RETURNING *, 1 AS marker")]
    fn returning_empty_star_preserves_group_before_fixed_output(#[case] sql: &str) {
        let analyzed = source_namespace_statement(sql);
        assert!(
            analyzed.context().reported_problems().is_empty(),
            "{:?}",
            analyzed.context().reported_problems()
        );
        let groups = analyzed.context().analysis_state().wildcard_expansions();
        assert_eq!(groups.len(), 1);
        assert!(groups[0].is_table_dependent());
        assert!(groups[0].columns().is_empty());
    }

    #[test]
    fn repeated_names_keep_their_resolved_relation_occurrence() {
        let analyzed = source_namespace_statement("SELECT a.t.*, b.t.* FROM a.t CROSS JOIN b.t");
        let groups = analyzed.context().analysis_state().wildcard_expansions();
        assert_eq!(groups.len(), 2);
        groups.assert_relation_schemas(&["a", "b"]);
    }

    #[test]
    fn set_operands_keep_separate_expansion_groups() {
        let analyzed = source_namespace_statement("SELECT * FROM a.u UNION ALL SELECT * FROM b.u");
        let groups = analyzed.context().analysis_state().wildcard_expansions();
        assert_eq!(groups.len(), 2);
        assert_eq!(
            groups[0].columns()[0]
                .column_name()
                .as_ref()
                .unwrap()
                .as_str(),
            "aid"
        );
        assert_eq!(
            groups[1].columns()[0]
                .column_name()
                .as_ref()
                .unwrap()
                .as_str(),
            "bid"
        );
    }

    #[test]
    fn duplicate_source_metadata_retains_columns_reordered_by_outer_using() {
        let analyzed = source_namespace_statement(
            "SELECT * FROM (a.t CROSS JOIN b.t CROSS JOIN a.u) j JOIN (SELECT 1 AS aid) v USING(aid)",
        );
        assert!(
            analyzed.context().reported_problems().is_empty(),
            "{:?}",
            analyzed.context().reported_problems()
        );
        let groups = analyzed.context().analysis_state().wildcard_expansions();
        let source = &groups[0].source_projections()[0];
        assert_eq!(source.alias(), "j");
        source
            .columns()
            .assert_retained_names("id,only_a,id,only_b,aid");
        assert_eq!(groups[0].source_ordinal_at(0), Some(("j", 4)));
        assert_eq!(groups[0].source_ordinal_at(1), Some(("j", 0)));
        assert_eq!(groups[0].source_ordinal_at(3), Some(("j", 2)));
    }

    #[test]
    fn unaliased_same_local_relations_retain_component_qualifiers() {
        let analyzed = source_namespace_statement("SELECT * FROM a.t FULL JOIN b.t USING(id)");
        let groups = analyzed.context().analysis_state().wildcard_expansions();
        let qualifiers = groups[0].qualifiers();
        assert_eq!(
            qualifiers
                .reference_for(
                    &crate::names::FullName::with_schema("a", "t"),
                    &crate::names::AliasName::from("t")
                )
                .unwrap(),
            ["a", "t"]
        );
        assert_eq!(
            qualifiers
                .reference_for(
                    &crate::names::FullName::with_schema("b", "t"),
                    &crate::names::AliasName::from("t")
                )
                .unwrap(),
            ["b", "t"]
        );
        assert_eq!(groups[0].reference_at(1).unwrap(), ["a", "t", "only_a"]);
    }

    #[test]
    fn nested_duplicate_sources_keep_alias_scopes_separate() {
        let analyzed =
            source_namespace_statement("SELECT j.* FROM ((a.t CROSS JOIN b.t) j CROSS JOIN a.u) j");
        assert!(
            analyzed.context().reported_problems().is_empty(),
            "{:?}",
            analyzed.context().reported_problems()
        );
        let groups = analyzed.context().analysis_state().wildcard_expansions();
        let outer = &groups[0].source_projections()[0];
        let inner = &outer.source_projections()[0];
        assert_eq!(outer.alias(), "j");
        assert_eq!(inner.alias(), "j");
        assert_eq!(outer.columns().len(), 5);
        assert_eq!(inner.columns().len(), 4);
        assert_eq!(outer.source_ordinal_at(0), Some(("j", 0)));
    }

    #[rstest::rstest]
    #[case::table("SELECT j.* FROM a.mixed j(x,x)")]
    #[case::function("SELECT j.* FROM mixed_fun() j(x,x)")]
    #[case::view("SELECT j.* FROM a.mixed_v j(x,x)")]
    fn duplicate_base_aliases_keep_physical_column_ordinals(#[case] sql: &str) {
        let analyzed = source_namespace_statement(sql);
        assert!(
            analyzed.context().reported_problems().is_empty(),
            "{:?}",
            analyzed.context().reported_problems()
        );
        let groups = analyzed.context().analysis_state().wildcard_expansions();
        assert_eq!(
            groups[0].columns()[0].col_type(),
            &Some(crate::sem::PgBuiltInType::int4())
        );
        assert_eq!(
            groups[0].columns()[1].col_type(),
            &Some(crate::sem::PgBuiltInType::int8())
        );
        assert_eq!(groups[0].typed_record_at(0).unwrap().column(), "id");
        assert_eq!(groups[0].typed_record_at(1).unwrap().column(), "other");
        assert_eq!(groups[0].typed_record_at(0).unwrap().alias(), "j");
    }

    #[test]
    fn hidden_duplicate_join_origin_does_not_escape_its_outer_alias() {
        let analyzed = source_namespace_statement(
            "SELECT k.* FROM ((a.t JOIN b.t ON true) j(id,id) JOIN a.u ON true) k(first,second,third)",
        );
        assert!(
            analyzed.context().reported_problems().is_empty(),
            "{:?}",
            analyzed.context().reported_problems()
        );
        let group = &analyzed.context().analysis_state().wildcard_expansions()[0];
        assert_eq!(group.source_ordinal_at(0), Some(("k", 0)));
        assert_eq!(
            group.reference_at(0),
            Some(["k".to_owned(), "first".to_owned()].as_slice())
        );
    }

    #[rstest::rstest]
    #[case::unknown_function("SELECT * FROM missing_wildcard_function()")]
    #[case::invalid_arguments("SELECT * FROM row_fun(1)")]
    #[case::rows_from("SELECT * FROM ROWS FROM (row_fun(1))")]
    fn unresolved_function_output_is_not_a_known_empty_projection(#[case] sql: &str) {
        let (_, context) = table_function_query(sql, false);
        assert!(
            context.has_reported_problem("query_reference_not_found"),
            "{:?}",
            context.reported_problems()
        );
    }
}

impl<'a> WildcardProjection<'a> {
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

#[cfg(test)]
mod test_row_source_star_expands_fields_before_positional_member_access {
    use crate::{context::ParseContext, test_helpers::query::source_namespace_statement};

    #[test]
    fn row_source_star_expands_fields_before_positional_member_access() {
        let analyzed = source_namespace_statement(
            "SELECT (ROW(j.*)).f1, (ROW(j.*)).f3 FROM (a.t CROSS JOIN b.t) j",
        );
        assert!(
            analyzed.context().reported_problems().is_empty(),
            "{:?}",
            analyzed.context().reported_problems()
        );
        let columns = analyzed.ast().result_columns().unwrap();
        assert_eq!(
            columns.get_column_at(0).unwrap().col_type(),
            &Some(crate::sem::PgBuiltInType::int4())
        );
        assert_eq!(
            columns.get_column_at(1).unwrap().col_type(),
            &Some(crate::sem::PgBuiltInType::int8())
        );
        assert!(
            analyzed
                .context()
                .analysis_state()
                .wildcard_expansions()
                .is_empty()
        );
    }
}
