use crate::sem::{
    AnalysisError, ColumnDefinition, ColumnName, SemScalarExpr, TypeReference,
    create_comp_type::{CompositeFieldCollection, CompositeFieldDefinition},
};

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ResultColumn {
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
            col_name: col_name.clone(),
            col_type: col_type.clone(),
            is_not_null: *is_not_null,
        }
    }
}

#[cfg(test)]
mod test_result_column_new {
    use super::ResultColumn;

    #[test]
    fn test_result_column_new() {
        let col = ResultColumn::new(&None, &None, &None);

        assert_eq!(col.column_name(), &None);
        assert_eq!(col.col_type(), &None);
        assert_eq!(col.is_not_null(), &None);
    }
}

impl ResultColumn {
    pub fn column_name(&self) -> &Option<ColumnName> {
        &self.col_name
    }

    pub fn col_type(&self) -> &Option<TypeReference> {
        &self.col_type
    }

    pub fn is_not_null(&self) -> &Option<bool> {
        &self.is_not_null
    }
}

impl ResultColumn {
    pub fn create_column_definition(&self) -> ColumnDefinition {
        ColumnDefinition::new(&self.col_name, self.col_type.as_ref(), self.is_not_null)
    }
}

#[cfg(test)]
mod test_result_column_create_column_definition {
    use super::ResultColumn;

    #[test]
    fn test_result_column_create_column_definition() {
        let col = ResultColumn::new(&None, &None, &None);
        let column_definition = col.create_column_definition();
        assert_eq!(column_definition.name(), &None);
        assert_eq!(column_definition.get_type(), None);
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
    use testresult::TestResult;

    use crate::sem::{ColumnName, TypeReference};

    use super::ResultColumn;

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
