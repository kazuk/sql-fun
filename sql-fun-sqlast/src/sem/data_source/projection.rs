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
    Failure(Box<SemScalarExpr>),
}

impl<'a> ProjectionBinding<'a> {
    const fn source(source: QualifiedSource<'a>) -> Self {
        Self::Source(source)
    }
    fn failure(expression: SemScalarExpr) -> Self {
        Self::Failure(Box::new(expression))
    }

    fn with_nullability(self, nullable: bool) -> Self {
        match self {
            Self::Source(source) => Self::Source(source.with_nullability(nullable)),
            Self::Failure(_) => self,
        }
    }
    fn with_merge(self, role: super::MergeRecordRole, nullable: bool) -> Self {
        match self {
            Self::Source(source) => Self::Source(source.with_merge(role, nullable)),
            Self::Failure(_) => self,
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
            Self::Failure(expression) => Ok(expression.as_ref().clone()),
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
            Self::Failure(expression) => Ok(expression.as_ref().clone()),
        }
    }
}

impl<'a> ProjectionColumn<'a> {
    fn new(source: QualifiedSource<'a>, name: ColumnName) -> Self {
        Self {
            column: name.clone(),
            name,
            alias: None,
            binding: ProjectionBinding::source(source),
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
            binding: ProjectionBinding::failure(failure),
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
            Self::Unknown(_, _) => Ok(Vec::new()),
            Self::MergeRecord(record) => record.column_names(context, with),
            Self::SubQuery(query) => Ok(query.select_statement().result_columns().names()),
            _ => AnalysisError::raise_unexpected_input(
                "star source has no enumerable result columns",
            ),
        }
    }
}
