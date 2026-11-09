use crate::{
    ast::{Dialect, Join},
    context::ParseContext,
    names::{AliasName, ColumnName, FullName},
    sem::{
        AliasSpec, AnalysisError, ColumnReferenceExpr, DataSource, JoinKind, Nullable,
        SemScalarExpr, WithClause,
        data_source::{
            ColumnCandidates, ProjectionColumn, QualifiedSource, SourceNamespace,
            projection::ProjectionColumns,
        },
        traversal::{SemanticVisitor, SemanticVisitorMut, optional, visit_optional},
    },
};

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct JoinUsingClause {
    using_columns: Vec<ColumnName>,
    alias: Option<AliasSpec>,
}

impl JoinUsingClause {
    pub(crate) fn new(using_columns: &[ColumnName], alias: &Option<AliasSpec>) -> Self {
        Self {
            using_columns: using_columns.to_owned(),
            alias: alias.clone(),
        }
    }

    fn contains_column(&self, column: &ColumnName) -> bool {
        self.using_columns.contains(column)
    }
}

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct JoinExprDataSource {
    kind: JoinKind,
    left: DataSource,
    right: DataSource,
    qualifications: Option<SemScalarExpr>,
    alias: Option<AliasSpec>,
    using_clause: Option<JoinUsingClause>,
    natural: bool,
}

impl JoinExprDataSource {
    pub(super) fn projection_shape_is_resolved(&self) -> bool {
        self.left.projection_shape_is_resolved() && self.right.projection_shape_is_resolved()
    }

    pub(super) fn projection_is_table_dependent(&self, context: &impl ParseContext) -> bool {
        self.left.projection_is_table_dependent(context)
            || self.right.projection_is_table_dependent(context)
    }

    pub(super) fn namespace<'a>(&'a self, source: &'a DataSource) -> SourceNamespace<'a> {
        if let Some(alias) = &self.alias {
            let mut namespace = SourceNamespace::default();
            namespace.push(source, alias);
            return namespace;
        }
        let mut namespace = self
            .left
            .namespace()
            .with_nullability(self.kind.left_nullable(false));
        namespace.append(
            self.right
                .namespace()
                .with_nullability(self.kind.right_nullable(false)),
        );
        namespace
    }
}

#[cfg(test)]
mod test_join_alias_hides_colliding_inner_occurrences {

    use crate::{names::FullName, test_helpers::query::source_namespace_statement};

    #[test]
    fn join_alias_hides_colliding_inner_occurrences() {
        let analyzed = source_namespace_statement("SELECT a.t.id FROM (a.t CROSS JOIN a.u) j, a.t");
        assert!(
            analyzed.context().reported_problems().is_empty(),
            "{:?}",
            analyzed.context().reported_problems()
        );
        let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
        assert_eq!(
            query
                .result_columns()
                .get_column_at(0)
                .unwrap()
                .expression()
                .unwrap()
                .table_column_reference()
                .unwrap()
                .relation(),
            &FullName::with_schema("a", "t")
        );
    }
}

impl JoinExprDataSource {
    pub(super) fn qualified_source<'a>(
        &'a self,
        context: &impl ParseContext,
        name: &FullName,
    ) -> Result<Option<QualifiedSource<'a>>, ()> {
        if self.alias.is_some() {
            return Ok(None);
        }
        let left = self.left.qualified_source(context, name)?;
        let right = self.right.qualified_source(context, name)?;
        match (left, right) {
            (Some(_), Some(_)) => Err(()),
            (Some(source), None) => Ok(Some(
                source.with_nullability(self.kind.left_nullable(false)),
            )),
            (None, Some(source)) => Ok(Some(
                source.with_nullability(self.kind.right_nullable(false)),
            )),
            (None, None) => Ok(None),
        }
    }

    pub(super) fn new_data_source(
        kind: &JoinKind,
        left: &DataSource,
        right: &DataSource,
        qualifications: &Option<SemScalarExpr>,
        alias: &Option<AliasSpec>,
        using_clause: &Option<JoinUsingClause>,
        natural: bool,
    ) -> DataSource {
        DataSource::JoinExpr(Box::new(Self {
            kind: kind.clone(),
            left: left.clone(),
            right: right.clone(),
            qualifications: qualifications.clone(),
            alias: alias.clone(),
            using_clause: using_clause.clone(),
            natural,
        }))
    }

    pub(super) fn push_alias_names(&self, names: &mut Vec<AliasSpec>) {
        if let Some(alias) = &self.alias {
            names.push(alias.clone());
        } else {
            self.left.push_alias_names(names);
            self.right.push_alias_names(names);
        }
    }

    pub(super) fn resolve_alias<'a>(
        &'a self,
        source: &'a DataSource,
        name: &AliasName,
    ) -> Option<(&'a DataSource, Nullable)> {
        if let Some(alias) = &self.alias {
            return if alias.name() == name {
                Some((source, false))
            } else {
                None
            };
        }
        if let Some((left_source, left_nullable)) = self.left.resolve_alias(name) {
            Some((left_source, self.kind.left_nullable(left_nullable)))
        } else {
            let (right_source, right_nullable) = self.right.resolve_alias(name)?;
            Some((right_source, self.kind.right_nullable(right_nullable)))
        }
    }

    pub(super) fn column_candidates<TParseContext: ParseContext>(
        &self,
        column: &ColumnName,
        context: &TParseContext,
        with_clause: &WithClause,
    ) -> ColumnCandidates {
        let mut candidates = self
            .left
            .aliases_for_column_resolved(column, context, with_clause);
        candidates.append(
            self.right
                .aliases_for_column_resolved(column, context, with_clause),
        );
        candidates
    }
}

impl JoinExprDataSource {
    pub(super) fn create_column_ref<'context, C: ParseContext>(
        &self,
        context: &'context mut C,
        with: &WithClause,
        column: &ColumnName,
        nullable: bool,
    ) -> Result<(SemScalarExpr, &'context mut C), AnalysisError> {
        if self.column_candidates(column, context, with).is_unknown() {
            let expression = ColumnReferenceExpr::undefined_column_resolved(context, column)?;
            return Ok((expression, context));
        }
        let columns = self.project(context, with, None)?;
        let mut selected = None;
        for candidate in columns {
            if candidate.name() == column {
                if selected.is_some() {
                    return Ok((
                        ColumnReferenceExpr::ambiguous_column_resolved(context, column)?,
                        context,
                    ));
                }
                selected = Some(candidate);
            }
        }
        let value = if let Some(selected) = selected {
            selected
                .with_nullability(nullable)
                .bind_direct(context, with)?
        } else {
            ColumnReferenceExpr::undefined_column_resolved(context, column)?
        };
        Ok((value, context))
    }
}

#[cfg(test)]
mod test_joined_alias_keeps_its_exposed_column_name {

    use crate::{sem::PgBuiltInType, test_helpers::query::source_namespace_statement};

    #[rstest::rstest]
    #[case::table("SELECT j.z FROM (a.t CROSS JOIN b.t) j(x,y,z,w)")]
    #[case::view("SELECT j.z FROM (a.t CROSS JOIN b.v) j(x,y,z)")]
    #[case::cte("WITH c AS (SELECT id FROM b.t) SELECT j.z FROM (a.t CROSS JOIN c) j(x,y,z)")]
    #[case::derived("SELECT j.z FROM (a.t CROSS JOIN (SELECT id FROM b.t) s) j(x,y,z)")]
    fn joined_alias_keeps_its_exposed_column_name(#[case] sql: &str) {
        let analyzed = source_namespace_statement(sql);
        assert!(
            analyzed.context().reported_problems().is_empty(),
            "{:?}",
            analyzed.context().reported_problems()
        );
        let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
        let column = query.result_columns().get_column_at(0).unwrap();
        assert_eq!(column.column_name().as_ref().unwrap().to_string(), "z");
        assert_eq!(column.expression().unwrap().get_column_name(), "z");
        assert_eq!(column.col_type(), &Some(PgBuiltInType::int8()));
    }

    #[test]
    fn a_join_alias_preserves_occurrence_identity() {
        let analyzed = source_namespace_statement(
            "SELECT j.only_a, t.only_a FROM (a.t CROSS JOIN a.u) j, a.t",
        );
        assert!(
            analyzed.context().reported_problems().is_empty(),
            "{:?}",
            analyzed.context().reported_problems()
        );
        let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
        for (index, alias) in [(0, "j"), (1, "t")] {
            let binding = query
                .result_columns()
                .get_column_at(index)
                .unwrap()
                .expression()
                .unwrap()
                .table_column_reference()
                .unwrap();
            assert_eq!(binding.alias().as_str(), alias);
        }
    }
}

impl JoinExprDataSource {
    pub(super) fn group_column_source<'a, C: ParseContext>(
        &'a self,
        column: &ColumnName,
        context: &C,
        with: &WithClause,
    ) -> Option<(&'a DataSource, Nullable)> {
        let left = self.left.group_column_source(column, context, with);
        // RIGHT JOIN preserves the right-hand USING or NATURAL column.
        if self.kind.uses_right_join_column()
            && left.is_some()
            && (self.natural
                || self
                    .using_clause
                    .as_ref()
                    .is_some_and(|using| using.contains_column(column)))
            && let Some((source, nullable)) = self.right.group_column_source(column, context, with)
        {
            return Some((source, self.kind.right_nullable(nullable)));
        }
        // Other duplicate column names select the first matching column.
        if let Some((source, nullable)) = left {
            Some((source, self.kind.left_nullable(nullable)))
        } else {
            let (source, nullable) = self.right.group_column_source(column, context, with)?;
            Some((source, self.kind.right_nullable(nullable)))
        }
    }
}

impl JoinExprDataSource {
    pub(crate) fn walk<V: SemanticVisitorMut>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        visitor.enter_join();
        self.left.walk(visitor)?;
        visitor.join_right();
        self.right.walk(visitor)?;
        visitor.leave_join(
            self.using_clause
                .as_ref()
                .map_or(&[], |using| using.using_columns.as_slice()),
            self.natural,
            self.alias.as_ref(),
        );
        optional(&mut self.qualifications, visitor)?;
        if let Some(value) = &self.qualifications {
            visitor.predicate(value);
        }
        Ok(())
    }
    pub(crate) fn visit<V: SemanticVisitor>(&self, visitor: &mut V) -> Result<(), AnalysisError> {
        visitor.enter_join();
        self.left.visit(visitor)?;
        visitor.join_right();
        self.right.visit(visitor)?;
        visitor.access_join(self);
        visitor.leave_join(
            self.using_clause
                .as_ref()
                .map_or(&[], |using| using.using_columns.as_slice()),
            self.natural,
            self.alias.as_ref(),
        );
        visit_optional(&self.qualifications, visitor)?;
        if let Some(value) = &self.qualifications {
            visitor.predicate(value);
        }
        visitor.leave_access_join();
        Ok(())
    }
}

impl JoinExprDataSource {
    pub(super) fn project(
        &self,
        context: &impl ParseContext,
        with: &WithClause,
        qualifier: Option<&AliasName>,
    ) -> Result<Vec<ProjectionColumn<'_>>, AnalysisError> {
        if self.matches_projection_qualifier(qualifier) {
            let columns = self.projection_names(context, with)?;
            return Ok(if let Some(alias) = &self.alias {
                ProjectionColumns::from(columns)
                    .with_join_alias(alias)
                    .into_columns()
            } else {
                columns
            });
        }
        if self.alias.is_some() {
            return Ok(Vec::new());
        }
        let mut columns = Vec::new();
        columns.extend(
            self.left
                .projection_names(context, with, qualifier)?
                .into_iter()
                .map(|column| column.with_nullability(self.kind.left_nullable(false))),
        );
        columns.extend(
            self.right
                .projection_names(context, with, qualifier)?
                .into_iter()
                .map(|column| column.with_nullability(self.kind.right_nullable(false))),
        );
        Ok(columns)
    }

    pub(super) fn matches_projection_qualifier(&self, qualifier: Option<&AliasName>) -> bool {
        qualifier.is_none_or(|name| {
            self.alias
                .as_ref()
                .is_some_and(|alias| alias.name() == name)
        })
    }

    pub(super) fn projection_names(
        &self,
        context: &impl ParseContext,
        with: &WithClause,
    ) -> Result<Vec<ProjectionColumn<'_>>, AnalysisError> {
        let original_left = self.left.projection_names(context, with, None)?;
        let original_right = self.right.projection_names(context, with, None)?;
        let layout = self.projection_layout(
            context.dialect(),
            &original_left
                .iter()
                .map(|column| column.name().clone())
                .collect::<Vec<_>>(),
            &original_right
                .iter()
                .map(|column| column.name().clone())
                .collect::<Vec<_>>(),
        );
        let left = original_left
            .iter()
            .cloned()
            .map(|column| column.with_nullability(self.kind.left_nullable(false)))
            .collect::<Vec<_>>();
        let right = original_right
            .iter()
            .cloned()
            .map(|column| column.with_nullability(self.kind.right_nullable(false)))
            .collect::<Vec<_>>();
        Ok(layout.project(&left, &right, |left, right| {
            ProjectionColumn::merged(&original_left[left], &original_right[right])
        }))
    }

    pub(crate) fn has_explicit_using_key(
        &self,
        context: &impl ParseContext,
        column: &ColumnName,
    ) -> bool {
        !self.natural
            && self.using_clause.as_ref().is_some_and(|using| {
                using.using_columns.iter().any(|key| {
                    if context.dialect() == Dialect::sqlite() {
                        key.as_str().eq_ignore_ascii_case(column.as_str())
                    } else {
                        key == column
                    }
                })
            })
    }
    pub(crate) const fn alias(&self) -> Option<&AliasSpec> {
        self.alias.as_ref()
    }

    // The same semantic output wiring serves column binding and access lineage.
    // No catalog lookup, type solving or expression interpretation occurs here.
    pub(crate) fn projection_layout(
        &self,
        dialect: Dialect,
        left: &[ColumnName],
        right: &[ColumnName],
    ) -> JoinProjection {
        let keys = if let Some(using) = &self.using_clause {
            using.using_columns.clone()
        } else if self.natural {
            left.iter()
                .filter(|name| right.contains(name))
                .cloned()
                .collect()
        } else {
            Vec::new()
        };
        let mut output = Vec::new();
        if dialect == Dialect::postgres() {
            for name in &keys {
                let left_index = left.iter().position(|column| column == name);
                let right_index = right.iter().position(|column| column == name);
                if self.kind.is_full() {
                    if let (Some(left), Some(right)) = (left_index, right_index) {
                        output.push(JoinColumnInput::both(left, right));
                    }
                } else if self.kind.uses_right_join_column() {
                    if let Some(right) = right_index {
                        output.push(JoinColumnInput::right(right));
                    }
                } else if let Some(left) = left_index {
                    output.push(JoinColumnInput::left(left));
                }
            }
            output.extend(
                left.iter()
                    .enumerate()
                    .filter(|(_, name)| !keys.contains(name))
                    .map(|(index, _)| JoinColumnInput::left(index)),
            );
        } else {
            for (index, name) in left.iter().enumerate() {
                if self.kind.is_full() && keys.contains(name) {
                    if let Some(left) = left.iter().position(|column| column == name)
                        && let Some(right) = right.iter().position(|column| column == name)
                    {
                        output.push(JoinColumnInput::both(left, right));
                    }
                } else {
                    output.push(JoinColumnInput::left(index));
                }
            }
        }
        output.extend(
            right
                .iter()
                .enumerate()
                .filter(|(_, name)| !keys.contains(name))
                .map(|(index, _)| JoinColumnInput::right(index)),
        );
        JoinProjection { keys, output }
    }
}

#[cfg(test)]
mod test_sqlite_full_join_using_keeps_left_position_and_right_fallback {
    #[cfg(feature = "sqlite")]
    use crate::{ast::Dialect, test_helpers::query::resolve};

    #[cfg(feature = "sqlite")]
    #[rstest::rstest]
    #[case::bare("SELECT * FROM (SELECT 1 AS id) l FULL JOIN (SELECT 2 AS id) r USING(id)")]
    #[case::grouped(
        "SELECT * FROM ((SELECT 1 AS id) l FULL JOIN (SELECT 2 AS id) r USING(id)) AS j"
    )]
    fn sqlite_full_join_using_keeps_left_position_and_right_fallback(#[case] sql: &str) {
        let (query, context) = resolve(sql, Dialect::sqlite(), false).unwrap();
        assert!(
            context.reported_problems().is_empty(),
            "{:?}",
            context.reported_problems()
        );
        let columns = query.result_columns();
        assert_eq!(columns.len(), 1);
        let key = columns.get_column_at(0).unwrap();
        assert!(key.expression().unwrap().as_coalesce().is_some());
        assert_eq!(key.column_name().as_ref().unwrap().as_str(), "id");
    }

    #[cfg(feature = "sqlite")]
    #[rstest::rstest]
    #[case::qualified("j.id")]
    #[case::unqualified("id")]
    fn sqlite_grouped_full_join_key_keeps_both_inputs(#[case] key: &str) {
        let (query, _) = resolve(
            &format!(
                "SELECT {key} FROM ((SELECT 1 AS id) l FULL JOIN (SELECT 2 AS id) r USING(id)) AS j"
            ),
            Dialect::sqlite(),
            false,
        )
        .unwrap();
        assert!(
            query
                .result_columns()
                .get_column_at(0)
                .unwrap()
                .expression()
                .unwrap()
                .as_coalesce()
                .is_some()
        );
    }
}

impl JoinExprDataSource {
    pub(super) fn projection_column_names(
        &self,
        context: &impl ParseContext,
        with: &WithClause,
    ) -> Result<Vec<ColumnName>, AnalysisError> {
        let columns = self.project(context, with, None)?;
        Ok(ProjectionColumns::from(columns).names())
    }
}

/// SQL output positions and the input positions contributing to each one.
pub(crate) struct JoinProjection {
    keys: Vec<ColumnName>,
    output: Vec<JoinColumnInput>,
}
impl JoinProjection {
    pub(crate) const fn keys(&self) -> &[ColumnName] {
        self.keys.as_slice()
    }
    pub(crate) fn project<T: Clone>(
        &self,
        left: &[T],
        right: &[T],
        merge: impl Fn(usize, usize) -> T,
    ) -> Vec<T> {
        self.output
            .iter()
            .map(|input| input.project(left, right, &merge))
            .collect()
    }
}
enum JoinColumnInput {
    Left(usize),
    Right(usize),
    Both(usize, usize),
}
impl JoinColumnInput {
    const fn left(index: usize) -> Self {
        Self::Left(index)
    }
    const fn right(index: usize) -> Self {
        Self::Right(index)
    }
    const fn both(left: usize, right: usize) -> Self {
        Self::Both(left, right)
    }
    fn project<T: Clone>(&self, left: &[T], right: &[T], merge: &impl Fn(usize, usize) -> T) -> T {
        match self {
            Self::Left(index) => left[*index].clone(),
            Self::Right(index) => right[*index].clone(),
            Self::Both(left, right) => merge(*left, *right),
        }
    }
}

impl JoinUsingClause {
    pub(crate) fn ast_using_clause(ast: &Join) -> Option<Self> {
        if ast.using().is_empty() {
            return None;
        }
        let mut columns = Vec::new();
        for column in ast.using() {
            columns.push(ColumnName::from(column.as_str()));
        }
        Some(Self::new(
            &columns,
            &ast.using_alias().as_ref().map(AliasSpec::from_ast),
        ))
    }
}
