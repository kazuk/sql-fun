mod ast;

use crate::{
    ast::Dialect,
    context::ParseContext,
    names::ColumnName,
    sem::{
        AnalysisError, AnalysisProblem, ColumnDefinition, ColumnReferenceExpr, IndirectionExpr,
        InputTypeConstraint, InsertTarget, ParameterConstraints, QueryProblem, SemScalarExpr,
        TypeReference,
        create_comp_type::{CompositeFieldCollection, CompositeFieldDefinition},
        parameter_resolution::ParameterResolution,
        source_flag::SourceFlag,
        traversal::{SemanticVisitor, SemanticVisitorMut},
    },
};

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
/// Metadata and retained expression of one result column.
pub struct ResultColumn {
    // Retain the semantic projection without changing serialized column metadata.
    #[serde(skip)]
    expression: Option<Box<SemScalarExpr>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    star_expansion: Option<StarExpansion>,
    #[serde(skip)]
    explicit_name: SourceFlag,
    #[serde(skip)]
    expanded: SourceFlag,
    #[serde(skip)]
    join_sensitive_star: SourceFlag,
    col_name: Option<ColumnName>,
    col_type: Option<TypeReference>,
    is_not_null: Option<bool>,
}

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
enum StarExpansion {
    Pending(SemScalarExpr),
    Invalid(SemScalarExpr),
}

impl StarExpansion {
    const fn with_expression(&self, expression: SemScalarExpr) -> Self {
        match self {
            Self::Pending(_) => Self::Pending(expression),
            Self::Invalid(_) => Self::Invalid(expression),
        }
    }
}

impl ResultColumn {
    pub(crate) fn resolved(name: &ColumnName, expression: &SemScalarExpr) -> Self {
        let mut column = Self::new(
            &Some(name.clone()),
            &expression.get_type(),
            &expression.is_not_null(),
        );
        column.expression = Some(Box::new(expression.clone()));
        column
    }

    /// Construct result-column metadata without a retained expression.
    pub fn new(
        col_name: &Option<ColumnName>,
        col_type: &Option<TypeReference>,
        is_not_null: &Option<bool>,
    ) -> Self {
        Self {
            explicit_name: SourceFlag::new(false),
            expanded: SourceFlag::new(false),
            join_sensitive_star: SourceFlag::new(false),
            expression: None,
            star_expansion: None,
            col_name: col_name.clone(),
            col_type: col_type.clone(),
            is_not_null: *is_not_null,
        }
    }
}

#[cfg(test)]
mod test_result_column_new {
    use crate::sem::select_statement::result_column::ResultColumn;

    #[test]
    fn test_result_column_new() {
        let col = ResultColumn::new(&None, &None, &None);

        assert_eq!(col.column_name(), &None);
        assert_eq!(col.col_type(), &None);
        assert_eq!(col.is_not_null(), &None);
    }
}

impl ResultColumn {
    /// Whether this result awaits a composite type before its output width is known.
    pub const fn has_pending_star(&self) -> bool {
        matches!(self.star_expansion, Some(StarExpansion::Pending(_)))
    }

    /// The retained name-resolved projection, when available.
    pub fn expression(&self) -> Option<&SemScalarExpr> {
        self.expression.as_deref()
    }

    /// The SQL result name, when known.
    pub const fn column_name(&self) -> &Option<ColumnName> {
        &self.col_name
    }

    /// The SQL result type, when known.
    pub const fn col_type(&self) -> &Option<TypeReference> {
        &self.col_type
    }

    /// The result nullability, when known.
    pub const fn is_not_null(&self) -> &Option<bool> {
        &self.is_not_null
    }
}

impl ResultColumn {
    /// Materialize this result as a column definition.
    pub fn create_column_definition(&self) -> ColumnDefinition {
        ColumnDefinition::new(
            &self.col_name,
            self.col_type.as_ref(),
            self.is_not_null,
            &[],
        )
    }
}

#[cfg(test)]
mod test_result_column_create_column_definition {
    use crate::sem::select_statement::result_column::ResultColumn;

    #[test]
    fn test_result_column_create_column_definition() {
        let col = ResultColumn::new(&None, &None, &None);
        let column_definition = col.create_column_definition();
        assert_eq!(column_definition.name(), &None);
        assert_eq!(column_definition.get_type_cloned(), None);
    }
}

impl ResultColumn {
    /// Materialize this result as a composite field.
    pub fn composite_field_definition(&self) -> Result<CompositeFieldDefinition, AnalysisError> {
        let Some(name) = &self.col_name else {
            AnalysisError::raise_unexpected_none("result_column.col_name")?;
            AnalysisError::never();
        };

        Ok(CompositeFieldDefinition::new(
            name.as_str(),
            &self.col_type,
            &self.is_not_null,
        ))
    }
}

#[cfg(test)]
mod test_result_column_composite_field_definition {
    use crate::{
        names::ColumnName,
        sem::{TypeReference, select_statement::result_column::ResultColumn},
    };

    use ::testresult::TestResult;

    #[test]
    fn test_composite_field_definition_err() {
        let col = ResultColumn::new(&None, &None, &None);
        let cfd = col.composite_field_definition();
        assert!(cfd.is_err())
    }

    #[test]
    fn test_composite_field_definition_normal() -> TestResult {
        let col = ResultColumn::new(
            &Some(ColumnName::from("col")),
            &Some(TypeReference::from_master_type_name("int4")),
            &Some(false),
        );
        let cfd = col.composite_field_definition()?;
        assert_eq!(cfd.name(), "col");
        Ok(())
    }
}

/// collection for result columns
#[derive(Default, Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ResultColumnCollection(Vec<ResultColumn>);

impl ResultColumnCollection {
    pub(crate) fn push_projection(
        &mut self,
        name: &ColumnName,
        expression: SemScalarExpr,
        explicit: bool,
    ) {
        self.push_expression(name, expression);
        self.0
            .last_mut()
            .expect("inserted projection")
            .explicit_name = SourceFlag::new(explicit);
    }

    pub(crate) fn membership_operands(&self) -> Vec<SemScalarExpr> {
        self.0
            .iter()
            .filter_map(ResultColumn::expression)
            .cloned()
            .collect()
    }

    pub(crate) fn cloned_suffix(&self, first: usize) -> Vec<ResultColumn> {
        self.0[first..].to_vec()
    }

    pub(crate) fn has_pending_stars(&self) -> bool {
        self.0.iter().any(ResultColumn::has_pending_star)
    }

    pub(crate) fn append_composite_star(
        &mut self,
        context: &mut impl ParseContext,
        base: SemScalarExpr,
    ) -> Result<(), AnalysisError> {
        let Some(_) = base.get_type() else {
            self.push_pending_star(base);
            return Ok(());
        };
        let Some(fields) = base.composite_columns(context) else {
            context.report_analysis_problem(AnalysisProblem::query(
                QueryProblem::star_projection_requires_composite_type(),
            ))?;
            self.push_invalid_star(base);
            return Ok(());
        };
        for name in fields.names() {
            let expression =
                IndirectionExpr::resolved_member(context, base.clone(), name.as_str())?;
            self.push_expanded(&name, expression);
        }
        Ok(())
    }

    pub(crate) fn push_pending_star(&mut self, base: SemScalarExpr) {
        let mut column = ResultColumn::new(&None, &None, &None);
        column.expression = Some(Box::new(base.clone()));
        column.star_expansion = Some(StarExpansion::Pending(base));
        self.0.push(column);
    }

    pub(crate) fn push_invalid_star(&mut self, base: SemScalarExpr) {
        let mut column = ResultColumn::new(&None, &None, &None);
        column.expression = Some(Box::new(base.clone()));
        column.star_expansion = Some(StarExpansion::Invalid(base));
        self.0.push(column);
    }

    pub(crate) fn expand_pending_stars(
        &mut self,
        context: &mut impl ParseContext,
    ) -> Result<bool, AnalysisError> {
        if !self.has_pending_stars() {
            return Ok(false);
        }
        let mut expanded = Self::default();
        let mut changed = false;
        for column in &self.0 {
            if !column.has_pending_star() {
                expanded.0.push(column.clone());
                continue;
            }
            let base = column.expression().expect("pending star base").clone();
            let previous_len = expanded.0.len();
            expanded.append_composite_star(context, base)?;
            changed |= expanded.0[previous_len..]
                .iter()
                .any(|column| !column.has_pending_star());
        }
        self.0 = expanded.0;
        Ok(changed)
    }

    pub(crate) fn validate_grouping_target(
        &mut self,
        index: usize,
        query_id: usize,
        context: &mut impl ParseContext,
    ) -> Result<(), AnalysisError> {
        if let Some(column) = self.0.get_mut(index)
            && let Some(expression) = &mut column.expression
        {
            expression.reject_grouping_key(query_id, context)?;
        }
        Ok(())
    }

    pub(crate) fn grouping_expression(&self, index: usize) -> Option<&SemScalarExpr> {
        self.0.get(index)?.expression()
    }
    pub(crate) fn default_returning_parameters(&self, parameters: &mut ParameterResolution<'_>) {
        for column in &self.0 {
            if let Some(value) = &column.expression {
                parameters.default_returning_parameter(value);
            }
        }
    }

    pub(crate) fn assign_update_output<V: SemanticVisitorMut>(
        &mut self,
        visitor: &mut V,
        index: usize,
        ty: &TypeReference,
    ) -> Result<(), AnalysisError> {
        if let Some(column) = self.0.get_mut(index)
            && let Some(value) = &mut column.expression
        {
            visitor.assignment_value(value, ty)?;
            column.col_type = value.get_type();
            column.is_not_null = value.is_not_null();
        }
        Ok(())
    }

    pub(crate) fn view_expressions(&self) -> Vec<(ColumnName, SemScalarExpr)> {
        let mut expressions = Vec::new();
        for column in &self.0 {
            if let Some(name) = column.column_name()
                && let Some(expression) = column.expression()
            {
                expressions.push((name.clone(), expression.clone()));
            }
        }
        expressions
    }
    pub(crate) fn view_projection(&self) -> Vec<(ColumnDefinition, Option<ColumnName>)> {
        self.0
            .iter()
            .map(|column| {
                (
                    column.create_column_definition(),
                    if let Some(expression) = column.expression() {
                        expression.view_source_column().cloned()
                    } else {
                        None
                    },
                )
            })
            .collect()
    }

    /// Number of result columns.
    pub const fn len(&self) -> usize {
        self.0.len()
    }
    /// Whether the result has no columns.
    pub const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub(crate) fn names(&self) -> Vec<ColumnName> {
        self.0
            .iter()
            .filter_map(|column| column.col_name.clone())
            .collect()
    }
    pub(crate) fn refresh_value(&mut self, index: usize, value: &SemScalarExpr) {
        if let Some(column) = self.0.get_mut(index) {
            column.col_type = value.get_type();
            column.is_not_null = value.is_not_null();
        }
    }

    pub(crate) fn resolve_output(
        &self,
        name: &ColumnName,
        context: &impl ParseContext,
    ) -> Result<Option<usize>, ()> {
        let mut found: Option<usize> = None;
        for (index, column) in self.0.iter().enumerate() {
            if column.col_name.as_ref() == Some(name) {
                if context.dialect() == Dialect::sqlite() {
                    return Ok(Some(index));
                }
                if let Some(previous) = found {
                    // PostgreSQL accepts duplicate aliases for identical expressions.
                    if self.0[previous].expression != column.expression {
                        return Err(());
                    }
                } else {
                    found = Some(index);
                }
            }
        }
        Ok(found)
    }
    pub(crate) fn output_reference(&self, index: usize) -> Option<SemScalarExpr> {
        let definition = self.0.get(index)?.create_column_definition();
        Some(SemScalarExpr::column_reference(
            ColumnReferenceExpr::output_column(index, &definition),
        ))
    }
    pub(crate) fn rename(&mut self, names: &[String]) {
        for (column, name) in self.0.iter_mut().zip(names) {
            column.col_name = Some(ColumnName::from(name.as_str()));
            column.explicit_name = SourceFlag::new(true);
        }
    }

    /// Look up result metadata by SQL column name.
    pub fn get_result_column_def(&self, column_name: &ColumnName) -> Option<ColumnDefinition> {
        self.0.iter().find_map(|c| {
            if c.col_name.as_ref() == Some(column_name) {
                Some(c.create_column_definition())
            } else {
                None
            }
        })
    }

    /// Whether a result with this SQL name exists.
    pub fn has_column(&self, column: &ColumnName) -> bool {
        self.0.iter().any(|rc| rc.col_name.as_ref() == Some(column))
    }

    /// Append metadata derived from an expression.
    pub fn push_column(&mut self, name: &ColumnName, value_expr: &SemScalarExpr) {
        let col_type = value_expr.get_type();
        self.0.push(ResultColumn::new(
            &Some(name.clone()),
            &col_type,
            &value_expr.is_not_null(),
        ));
    }

    pub(crate) fn can_replace_view_columns(&self, previous: &Self) -> bool {
        self.0.len() >= previous.0.len()
            && self.0.iter().zip(&previous.0).all(|(new, old)| {
                new.column_name() == old.column_name() && new.col_type() == old.col_type()
            })
    }

    /// Materialize all results as column definitions in order.
    pub fn column_definitions(&self) -> Result<Vec<ColumnDefinition>, AnalysisError> {
        let mut columns = Vec::new();
        for column in &self.0 {
            let Some(_col_name) = column.column_name() else {
                AnalysisError::raise_unexpected_none("ResultColumn.column_name")?
            };
            columns.push(column.create_column_definition());
        }
        Ok(columns)
    }

    /// Look up result metadata by zero-based position.
    pub fn get_column_at(&self, index: usize) -> Option<&ResultColumn> {
        self.0.get(index)
    }

    /// Look up a SQL name after skipping the first results.
    pub fn get_column_with_skip_count(
        &self,
        name: &ColumnName,
        skip_count: usize,
    ) -> Option<&ResultColumn> {
        self.0
            .get(skip_count..)
            .unwrap_or_default()
            .iter()
            .find(|v| v.col_name.as_ref() == Some(name))
    }

    /// Materialize these results as composite fields.
    pub fn composite_field_collection(&self) -> Result<CompositeFieldCollection, AnalysisError> {
        let mut fields = CompositeFieldCollection::default();
        for result_column in &self.0 {
            let field_def = result_column.composite_field_definition()?;
            fields.push(field_def);
        }
        Ok(fields)
    }
}

impl ResultColumnCollection {
    pub(crate) fn walk<V: SemanticVisitorMut>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        self.walk_assigned(visitor, &[])
    }
    pub(crate) fn visit<V: SemanticVisitor>(&self, visitor: &mut V) -> Result<(), AnalysisError> {
        self.visit_assigned(visitor, &[])
    }
    pub(crate) fn walk_assigned<V: SemanticVisitorMut>(
        &mut self,
        visitor: &mut V,
        targets: &[InsertTarget],
    ) -> Result<(), AnalysisError> {
        let pending = self.has_pending_stars();
        for (index, column) in self.0.iter_mut().enumerate() {
            if let Some(expression) = &mut column.expression {
                if column.join_sensitive_star.get() {
                    visitor.join_sensitive_star();
                }
                visitor.projection(
                    &mut column.col_name,
                    column.explicit_name.get(),
                    column.expanded.get(),
                    expression,
                )?;
                expression.walk(visitor)?;
                if let Some(star) = &mut column.star_expansion {
                    *star = star.with_expression(expression.as_ref().clone());
                    continue;
                }
                if pending {
                    continue;
                }
                if let Some(target) = targets.get(index) {
                    visitor.assignment(expression, target)?;
                }
                visitor.result_name(&mut column.col_name, column.explicit_name.get(), expression);
                visitor.output(
                    column.col_name.as_ref().map_or("", ColumnName::as_str),
                    expression,
                );
                column.col_type = expression.get_type();
                column.is_not_null = expression.is_not_null();
            }
        }
        Ok(())
    }
    pub(crate) fn visit_assigned<V: SemanticVisitor>(
        &self,
        visitor: &mut V,
        targets: &[InsertTarget],
    ) -> Result<(), AnalysisError> {
        let pending = self.has_pending_stars();
        for (index, column) in self.0.iter().enumerate() {
            if let Some(expression) = &column.expression {
                if column.join_sensitive_star.get() {
                    visitor.join_sensitive_star();
                }
                visitor.projection(
                    &column.col_name,
                    column.explicit_name.get(),
                    column.expanded.get(),
                    expression,
                )?;
                expression.visit(visitor)?;
                if column.star_expansion.is_some() {
                    continue;
                }
                if pending {
                    continue;
                }
                if let Some(target) = targets.get(index) {
                    visitor.assignment(expression, target)?;
                }
                visitor.result_name(&column.col_name, column.explicit_name.get(), expression);
                visitor.output(
                    column.col_name.as_ref().map_or("", ColumnName::as_str),
                    expression,
                );
            }
        }
        Ok(())
    }
}

impl ResultColumnCollection {
    pub(crate) fn mark_join_sensitive_star_since(&mut self, start: usize) {
        for column in &mut self.0[start..] {
            column.join_sensitive_star = SourceFlag::new(true);
        }
    }
    pub(crate) fn push_expanded(&mut self, name: &ColumnName, expression: SemScalarExpr) {
        self.push_expression(name, expression);
        self.0.last_mut().expect("inserted expansion").expanded = SourceFlag::new(true);
    }
    pub(crate) fn push_expression(&mut self, name: &ColumnName, expression: SemScalarExpr) {
        let mut column = ResultColumn::new(
            &Some(name.clone()),
            &expression.get_type(),
            &expression.is_not_null(),
        );
        column.expression = Some(Box::new(expression));
        self.0.push(column);
    }
}

#[cfg(test)]
mod test_projection_metadata_contract {
    use crate::{
        names::ColumnName,
        sem::{
            PgBuiltInType, ScalarConstExpr, SemScalarExpr,
            select_statement::result_column::{ResultColumn, ResultColumnCollection},
        },
    };

    #[test]
    fn retained_projection_preserves_serialized_column_metadata() {
        let name = ColumnName::from("n");
        let expression = SemScalarExpr::literal(&ScalarConstExpr::new_integer(1));
        let mut columns = ResultColumnCollection::default();
        columns.push_expression(&name, expression);
        let retained = columns.get_column_at(0).unwrap();
        assert!(retained.expression.is_some());
        let metadata = ResultColumn::new(&Some(name), &Some(PgBuiltInType::int4()), &Some(true));
        assert_eq!(
            ::serde_json::to_value(retained).unwrap(),
            ::serde_json::to_value(metadata).unwrap()
        );
    }
}

impl ResultColumnCollection {
    pub(crate) fn scalar_type(&self) -> Option<TypeReference> {
        match self.0.as_slice() {
            [column] => column.col_type.clone(),
            _ => None,
        }
    }
}

impl ResultColumnCollection {
    // A set boundary consumes the operand's SQL type before unknown literals
    // acquire a standalone projection's text representation.
    pub(crate) fn set_operand(&self, index: usize) -> Option<SemScalarExpr> {
        let column = self.0.get(index)?;
        if let Some(expression) = column.expression() {
            Some(expression.clone())
        } else {
            self.output_reference(index)
        }
    }

    pub(crate) fn retain_set_names(&mut self, left: &Self) {
        for (index, source) in left.0.iter().enumerate() {
            if let Some(column) = self.0.get_mut(index) {
                if !column.explicit_name.get() {
                    column.col_name = source.col_name.clone();
                }
            } else {
                self.0
                    .push(ResultColumn::new(&source.col_name, &None, &None));
            }
        }
        self.0.truncate(left.len());
    }

    pub(crate) fn set_output(
        &mut self,
        index: usize,
        types: &InputTypeConstraint,
        not_null: Option<bool>,
        expression: Option<SemScalarExpr>,
    ) {
        if let Some(column) = self.0.get_mut(index) {
            column.col_type = types.single_type();
            column.is_not_null = not_null;
            if let Some(expression) = expression {
                column.expression = Some(Box::new(expression));
            } else if column.expression.is_none() {
                // Assignment checks also walk parameter-free set results.
                column.expression = Some(Box::new(SemScalarExpr::column_reference(
                    ColumnReferenceExpr::output_column(index, &column.create_column_definition()),
                )));
            }
        }
    }

    pub(crate) fn finalize_set_output(
        &mut self,
        index: usize,
        constraints: &ParameterConstraints,
        not_null: Option<bool>,
    ) {
        if let Some(column) = self.0.get_mut(index) {
            if let Some(expression) = &mut column.expression {
                expression.apply_constraints(constraints);
                column.col_type = expression.get_type();
            }
            column.is_not_null = not_null;
        }
    }

    pub(crate) fn walk_set_outputs<V: SemanticVisitorMut>(
        &mut self,
        visitor: &mut V,
        targets: &[InsertTarget],
    ) -> Result<(), AnalysisError> {
        for (index, column) in self.0.iter_mut().enumerate() {
            if let Some(expression) = &mut column.expression {
                if let Some(target) = targets.get(index) {
                    visitor.assignment(expression, target)?;
                }
                visitor.output(
                    column.col_name.as_ref().map_or("", ColumnName::as_str),
                    expression,
                );
                column.col_type = expression.get_type();
            }
        }
        Ok(())
    }
    pub(crate) fn visit_set_outputs<V: SemanticVisitor>(
        &self,
        visitor: &mut V,
        targets: &[InsertTarget],
    ) -> Result<(), AnalysisError> {
        for (index, column) in self.0.iter().enumerate() {
            if let Some(expression) = &column.expression {
                if let Some(target) = targets.get(index) {
                    visitor.assignment(expression, target)?;
                }
                visitor.set_output(
                    index,
                    column.col_name.as_ref().map_or("", ColumnName::as_str),
                    expression,
                );
            }
        }
        Ok(())
    }
}
