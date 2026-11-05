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
}

impl<'a> QualifiedSource<'a> {
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

#[cfg(test)]
mod test_selected_occurrence_keeps_join_nullability {

    use crate::{
        names::FullName, sem::PgBuiltInType, test_helpers::query::source_namespace_statement,
    };

    #[rstest::rstest]
    #[case::left("SELECT t.* FROM a.u LEFT JOIN b.t ON true", false)]
    #[case::right("SELECT t.* FROM b.t RIGHT JOIN a.u ON true", false)]
    #[case::inner("SELECT t.* FROM a.u CROSS JOIN b.t", true)]
    #[case::unique("SELECT only_b FROM a.t LEFT JOIN b.t ON true", false)]
    fn selected_occurrence_keeps_join_nullability(#[case] sql: &str, #[case] not_null: bool) {
        let analyzed = source_namespace_statement(sql);
        assert!(
            analyzed.context().reported_problems().is_empty(),
            "{:?}",
            analyzed.context().reported_problems()
        );
        let query = analyzed.ast().clone().into_select_statemtnt().unwrap();
        let column = query.result_columns().get_column_at(0).unwrap();
        assert_eq!(column.col_type(), &Some(PgBuiltInType::int8()));
        assert_eq!(column.is_not_null(), &Some(not_null));
        assert_eq!(
            column
                .expression()
                .unwrap()
                .table_column_reference()
                .unwrap()
                .relation(),
            &FullName::with_schema("b", "t")
        );
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

#[cfg(test)]
mod test_resolves_nominal_arguments {
    use crate::{
        names::FullName,
        sem::{PgBuiltInType, TypeReference},
        test_helpers::{
            query::expression, query_whole_rows as test_helpers, query_whole_rows::catalog_query,
        },
    };

    #[rstest::rstest]
    #[case::positional(
        "SELECT accept_rows(a.*, t.*) FROM pg_catalog.pg_attribute a CROSS JOIN pg_catalog.pg_type t"
    )]
    #[case::parenthesized(
        "SELECT accept_rows((a.*), (t.*)) FROM pg_catalog.pg_attribute a CROSS JOIN pg_catalog.pg_type t"
    )]
    fn resolves_nominal_arguments(#[case] sql: &str) {
        let (query, context) = catalog_query(sql);
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:?}",
            context.reported_problems()
        );
        let call = expression(&query).as_function_call().unwrap();
        assert!(call.overload().is_some());
        assert_eq!(
            expression(&query).get_type(),
            Some(TypeReference::from_master_type_name("oid"))
        );
        for (index, (alias, relation)) in [("a", "pg_attribute"), ("t", "pg_type")]
            .into_iter()
            .enumerate()
        {
            let argument = &call.args()[index];
            let row = argument.value().as_whole_row().unwrap();
            assert_eq!(row.alias().unwrap().as_str(), alias);
            assert_eq!(
                row.relation(),
                Some(&FullName::with_schema("pg_catalog", relation))
            );
            assert_eq!(row.levels_up(), Some(0));
            assert!(row.failure_code().is_none());
            assert_eq!(
                argument.value().get_type(),
                Some(TypeReference::concrete_type_ref(
                    &FullName::with_schema("pg_catalog", relation),
                    false
                ))
            );
            assert!(argument.arg_definition().is_some());
        }
    }

    #[rstest::rstest]
    #[case::aliased("SELECT type_arg(t.*) FROM pg_catalog.pg_type t", false)]
    #[case::qualified("SELECT type_arg(pg_catalog.pg_type.*) FROM pg_catalog.pg_type", false)]
    #[case::outer(
        "SELECT type_arg(t.*) FROM pg_catalog.pg_attribute a LEFT JOIN pg_catalog.pg_type t ON false",
        true
    )]
    fn retains_relation_and_source_nullability(#[case] sql: &str, #[case] nullable: bool) {
        let (query, context) = catalog_query(sql);
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:?}",
            context.reported_problems()
        );
        let value = expression(&query).as_function_call().unwrap().args()[0].value();
        assert_eq!(
            value.get_type(),
            Some(TypeReference::concrete_type_ref(
                &FullName::with_schema("pg_catalog", "pg_type"),
                false
            ))
        );
        assert_eq!(value.is_not_null(), Some(!nullable));
    }

    #[test]
    fn derived_whole_row_retains_field_names_and_parameter_types() {
        let (query, context) = catalog_query("SELECT (x.*).n FROM (SELECT $1::integer AS n) x");
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:?}",
            context.reported_problems()
        );
        assert_eq!(expression(&query).get_type(), Some(PgBuiltInType::int4()));
        assert_eq!(test_helpers::row_levels(query), vec![0]);
    }

    #[rstest::rstest]
    #[case::members("SELECT (a.*).x, (b.*).y, (v.*).n FROM v AS a(x), v AS b(y), v")]
    #[case::stars("SELECT (a.*).*, (b.*).*, (v.*).* FROM v AS a(x), v AS b(y), v")]
    fn occurrence_aliases_do_not_modify_other_occurrences(#[case] sql: &str) {
        let (query, context) = test_helpers::alias_query(sql);
        assert!(context.problems().is_empty(), "{:?}", context.problems());
        for index in 0..3 {
            let column = query.result_columns().get_column_at(index).unwrap();
            // Names of star outputs are covered separately; this checks each
            // occurrence's retained source binding and selected field type.
            assert_eq!(column.col_type(), &Some(PgBuiltInType::int4()));
        }
        test_helpers::assert_catalog_view_field(&context);
        assert_eq!(test_helpers::row_levels(query), vec![0, 0, 0]);
    }

    #[rstest::rstest]
    #[case::view("SELECT (r.*).* FROM w AS r(x)")]
    #[case::derived("SELECT (r.*).* FROM (SELECT 1 AS n, 'a'::text AS label) AS r(x)")]
    fn partial_alias_list_preserves_field_order_and_types(#[case] sql: &str) {
        let (query, context) = test_helpers::alias_query(sql);
        assert!(context.problems().is_empty(), "{:?}", context.problems());
        for (index, (name, ty)) in [
            ("x", PgBuiltInType::int4()),
            ("label", PgBuiltInType::text()),
        ]
        .into_iter()
        .enumerate()
        {
            let column = query.result_columns().get_column_at(index).unwrap();
            assert_eq!(column.column_name().as_ref().unwrap().as_str(), name);
            assert_eq!(column.col_type(), &Some(ty));
        }
        assert!(query.result_columns().get_column_at(2).is_none());
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
}

#[cfg(test)]
mod test_scalar_function_source_keeps_its_scalar_type {
    use crate::{
        sem::PgBuiltInType,
        test_helpers::{query::expression, query_whole_rows::catalog_query},
    };

    #[test]
    fn scalar_function_source_keeps_its_scalar_type() {
        let (query, context) = catalog_query("SELECT identity(x.*) FROM identity(1) x");
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:?}",
            context.reported_problems()
        );
        let call = expression(&query).as_function_call().unwrap();
        assert!(call.overload().is_some());
        assert_eq!(
            call.args()[0].value().get_type(),
            Some(PgBuiltInType::int4())
        );
        let (column_query, _) = catalog_query("SELECT x FROM identity(1) x");
        assert_eq!(
            call.args()[0].value().is_not_null(),
            expression(&column_query).is_not_null()
        );
    }
}

impl DataSource {
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
