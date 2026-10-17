use crate::sem::{
    AnalysisError, ColumnDefinition, ColumnName, SemScalarExpr, TypeReference,
    create_comp_type::{CompositeFieldCollection, CompositeFieldDefinition},
};

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ResultColumn {
    // Retain the semantic projection without changing serialized column metadata.
    #[serde(skip)]
    expression: Option<Box<SemScalarExpr>>,
    col_name: Option<ColumnName>,
    col_type: Option<TypeReference>,
    is_not_null: Option<bool>,
}

impl ResultColumn {
    pub fn new(
        col_name: &Option<ColumnName>,
        col_type: &Option<TypeReference>,
        is_not_null: &Option<bool>,
    ) -> Self {
        Self {
            expression: None,
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
    pub const fn column_name(&self) -> &Option<ColumnName> {
        &self.col_name
    }

    pub const fn col_type(&self) -> &Option<TypeReference> {
        &self.col_type
    }

    pub const fn is_not_null(&self) -> &Option<bool> {
        &self.is_not_null
    }
}

impl ResultColumn {
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
    use crate::sem::select_statement::result_column::ResultColumn;
    use crate::sem::{ColumnName, TypeReference};

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
    pub(crate) fn refresh_value(&mut self, index: usize, value: &SemScalarExpr) {
        if let Some(column) = self.0.get_mut(index) {
            column.col_type = value.get_type();
            column.is_not_null = value.is_not_null();
        }
    }

    pub(crate) fn resolve_output(
        &self,
        name: &ColumnName,
        context: &impl crate::sem::ParseContext,
    ) -> Result<Option<usize>, ()> {
        let mut found: Option<usize> = None;
        for (index, column) in self.0.iter().enumerate() {
            if column.col_name.as_ref() == Some(name) {
                if context.dialect() == crate::ast::Dialect::sqlite() {
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
            crate::sem::ColumnReferenceExpr::output_column(index, &definition),
        ))
    }
    pub(crate) fn rename(&mut self, names: &[String]) {
        for (column, name) in self.0.iter_mut().zip(names) {
            column.col_name = Some(ColumnName::from(name.as_str()));
        }
    }

    pub fn get_result_column_def(&self, column_name: &ColumnName) -> Option<ColumnDefinition> {
        self.0.iter().find_map(|c| {
            if c.col_name.as_ref() == Some(column_name) {
                Some(c.create_column_definition())
            } else {
                None
            }
        })
    }

    pub fn has_column(&self, column: &ColumnName) -> bool {
        self.0.iter().any(|rc| rc.col_name.as_ref() == Some(column))
    }

    pub fn push_column(&mut self, name: &ColumnName, value_expr: &SemScalarExpr) {
        let col_type = value_expr.get_type();
        self.0.push(ResultColumn::new(
            &Some(name.clone()),
            &col_type,
            &value_expr.is_not_null(),
        ));
    }

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

    pub fn get_column_at(&self, index: usize) -> Option<&ResultColumn> {
        self.0.get(index)
    }

    pub fn get_column_with_skip_count(
        &self,
        name: &ColumnName,
        skip_count: usize,
    ) -> Option<&ResultColumn> {
        self.0[skip_count..]
            .iter()
            .find(|v| v.col_name.as_ref() == Some(name))
    }

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
    pub(crate) fn walk<V: crate::sem::traversal::SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), crate::sem::AnalysisError> {
        for column in &mut self.0 {
            if let Some(expression) = &mut column.expression {
                expression.walk(visitor)?;
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
}

impl ResultColumnCollection {
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
    use super::{ResultColumn, ResultColumnCollection};
    use crate::sem::{ColumnName, PgBuiltInType, ScalarConstExpr, SemScalarExpr};

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
