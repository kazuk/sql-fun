use crate::sem::data_source::QualifiedSource;
use crate::sem::{
    AliasName, AnalysisError, ColumnName, DataSource, ParseContext, ScalarAnalysisContext,
    SemScalarExpr, WithClause,
};

/// An expanded column keeps the selected occurrence until its expression is bound.
#[derive(Debug, Clone)]
pub(crate) struct ProjectionColumn<'a> {
    name: ColumnName,
    column: ColumnName,
    alias: Option<AliasName>,
    binding: ProjectionBinding<'a>,
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

impl MergedProjection<'_> {
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
        crate::sem::CoalesceExpr::resolve_join_key(
            context,
            &[left, right],
            self.left.name(),
            not_null && !self.nullable,
        )
    }
}

impl<'a> ProjectionBinding<'a> {
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
            Self::Merged(mut merged) => {
                merged.nullable |= nullable;
                Self::Merged(merged)
            }
        }
    }
    fn with_merge(self, role: super::MergeRecordRole, nullable: bool) -> Self {
        match self {
            Self::Source(source) => Self::Source(source.with_merge(role, nullable)),
            Self::Expression(_) => self,
            Self::Merged(_) => self,
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
    pub(super) const fn is_merged(&self) -> bool {
        matches!(self.binding, ProjectionBinding::Merged(_))
    }
    fn new(source: QualifiedSource<'a>, name: ColumnName) -> Self {
        Self {
            column: name.clone(),
            name,
            alias: None,
            binding: ProjectionBinding::source(source),
        }
    }
    pub(super) fn expression(name: &ColumnName, expression: &SemScalarExpr) -> Self {
        Self {
            name: name.clone(),
            column: name.clone(),
            alias: None,
            binding: ProjectionBinding::expression(expression.clone()),
        }
    }
    pub(super) fn merged(left: &Self, right: &Self) -> Self {
        Self {
            name: left.name.clone(),
            column: left.name.clone(),
            alias: None,
            binding: ProjectionBinding::Merged(Box::new(MergedProjection {
                left: Box::new(left.clone()),
                right: Box::new(right.clone()),
                nullable: false,
            })),
        }
    }
    pub(crate) fn ambiguous(
        context: &mut impl ParseContext,
        qualifier: &AliasName,
    ) -> Result<Self, AnalysisError> {
        let name = ColumnName::from("*");
        let failure = crate::sem::QualifiedColumnFailure::ambiguous(
            context,
            &crate::sem::FullName::from_local_name(qualifier.as_str()),
            &name,
        )?;
        Ok(Self {
            column: name.clone(),
            name,
            alias: None,
            binding: ProjectionBinding::expression(failure),
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
        Ok(self.relabel(expression))
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
    pub(super) fn with_name(mut self, name: &ColumnName) -> Self {
        self.name = name.clone();
        self
    }
    pub(super) fn bind_direct(
        &self,
        context: &mut impl ParseContext,
        with: &WithClause,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let expression = self.binding.bind_direct(context, with, &self.column)?;
        Ok(self.relabel(expression))
    }
    pub(super) fn with_nullability(mut self, nullable: bool) -> Self {
        self.binding = self.binding.with_nullability(nullable);
        self
    }
    pub(super) fn with_merge(mut self, role: super::MergeRecordRole, nullable: bool) -> Self {
        self.binding = self.binding.with_merge(role, nullable);
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
            if qualifier.is_some_and(|name| !source.matches_alias(name)) {
                return Ok(Vec::new());
            }
            return Ok(source.project(context, with)?.into_columns());
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
            let names = source.column_names(context, with)?;
            for (index, name) in names.into_iter().enumerate() {
                let name = alias.derived_column_at(index).cloned().unwrap_or(name);
                columns.push(ProjectionColumn::new(
                    QualifiedSource::new(source, alias.name()).with_nullability(nullable),
                    name,
                ));
            }
        }
        Ok(columns)
    }
    pub(super) fn column_names(
        &self,
        context: &impl ParseContext,
        with: &WithClause,
    ) -> Result<Vec<ColumnName>, AnalysisError> {
        match self {
            Self::Table(table) => {
                let Some(table) = context.get_table(table.name()) else {
                    return AnalysisError::raise_unexpected_input("star relation missing");
                };
                let mut names = Vec::new();
                let mut index = 0;
                while let Some(column) = table.get_column_at(index) {
                    index += 1;
                    if let Some(name) = column.name() {
                        names.push(name.clone());
                    }
                }
                Ok(names)
            }
            Self::CteQuery(cte) => {
                let Some(cte) = with.get(cte.name()) else {
                    return AnalysisError::raise_unexpected_none("star CTE");
                };
                let Some(columns) = cte.result_columns() else {
                    return AnalysisError::raise_unexpected_none("star CTE columns");
                };
                Ok(columns.names())
            }
            Self::View(view) => {
                let Some(view) = context.get_view(view.view_name()) else {
                    return AnalysisError::raise_unexpected_none("star view");
                };
                Ok(view.result_columns().names())
            }
            Self::JoinExpr(join) => Ok(join
                .project(context, with, None)?
                .into_iter()
                .map(|column| column.name().clone())
                .collect()),
            Self::Group(group) => Ok(group
                .projection_names(self, context, with)?
                .into_iter()
                .map(|column| column.name().clone())
                .collect()),
            Self::Unknown(_, _) | Self::UndefinedTableFunc(_) => Ok(Vec::new()),
            Self::CallTableFunc(source) => Ok(source.column_names()),
            Self::DeferredTableFunc(source) => Ok(source.column_names()),
            Self::RowFunctions(source) => source.column_names(context, with),
            Self::MergeRecord(record) => record.column_names(context, with),
            Self::SubQuery(query) => Ok(query.select_statement().result_columns().names()),
            _ => AnalysisError::raise_unexpected_input(
                "star source has no enumerable result columns",
            ),
        }
    }
}

/// Projection occurrences retain order and duplicates until column binding.
#[derive(Default)]
pub(super) struct ProjectionColumns<'a>(Vec<ProjectionColumn<'a>>);

impl<'a> ProjectionColumns<'a> {
    pub(super) fn append_source(
        &mut self,
        source: &'a DataSource,
        context: &impl ParseContext,
        with: &WithClause,
    ) -> Result<(), AnalysisError> {
        self.0.extend(source.projection_names(context, with, None)?);
        Ok(())
    }
    pub(super) fn append_ordinality(&mut self, alias: &crate::sem::AliasSpec) {
        let name = ColumnName::from("ordinality");
        let definition = crate::sem::ColumnDefinition::new(
            &Some(name.clone()),
            Some(&crate::sem::PgBuiltInType::int8()),
            Some(true),
            &[],
        );
        let expression = crate::sem::ColumnReferenceExpr::function_output(alias, &definition);
        self.0
            .push(ProjectionColumn::expression(&name, &expression));
    }
    pub(super) fn with_aliases(self, alias: &crate::sem::AliasSpec) -> Self {
        Self(
            self.0
                .into_iter()
                .enumerate()
                .map(|(index, column)| {
                    let column = if let Some(name) = alias.derived_column_at(index) {
                        column.with_name(name)
                    } else {
                        column
                    };
                    column.with_join_alias(alias.name())
                })
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
            return Ok(crate::sem::ColumnReferenceExpr::function_source_ref(
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
            return crate::sem::ColumnReferenceExpr::ambiguous_column_resolved(context, name);
        }
        column.with_nullability(nullable).bind_direct(context, with)
    }
}
