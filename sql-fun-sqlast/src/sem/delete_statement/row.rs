use crate::{
    names::ColumnName,
    sem::{ColumnDefinition, ColumnReferenceExpr, TypeReference},
};

/// Row image bound by a PostgreSQL 18 DELETE RETURNING reference.
#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum DeleteRowVersion {
    /// Values before deletion.
    Old,
    /// Values after deletion (nullable for DELETE).
    New,
}
impl DeleteRowVersion {
    /// Select the values before deletion.
    pub const fn old() -> Self {
        Self::Old
    }
    /// Select the nullable values after deletion.
    pub const fn new_image() -> Self {
        Self::New
    }
    const fn is_new(&self) -> bool {
        matches!(self, Self::New)
    }

    pub(crate) fn from_alias(alias: &str) -> Option<Self> {
        match alias {
            "old" => Some(Self::Old),
            "new" => Some(Self::New),
            _ => None,
        }
    }
}

/// A resolved DELETE row-image column, retaining its underlying catalog binding.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DeleteRowColumn {
    version: DeleteRowVersion,
    source: Box<ColumnReferenceExpr>,
}
impl DeleteRowColumn {
    pub(crate) fn new(version: DeleteRowVersion, source: &ColumnReferenceExpr) -> Self {
        Self {
            version,
            source: Box::new(source.clone()),
        }
    }
}

#[cfg(test)]
mod test_row_images_preserve_typed_catalog_bindings {
    use crate::{
        ast::Dialect,
        test_helpers::{delete::resolve, delete_versions as test_helpers},
    };

    #[rstest::rstest]
    #[case::old_star("DELETE FROM items RETURNING old.*", 2)]
    #[case::new_star("DELETE FROM items RETURNING new.*", 2)]
    #[case::nested("DELETE FROM items RETURNING (SELECT old.id)", 1)]
    fn row_images_preserve_typed_catalog_bindings(#[case] sql: &str, #[case] width: usize) {
        let mut context = test_helpers::catalog_for_version("18");
        let result = resolve(sql, &mut context, Dialect::postgres()).unwrap();
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:?}",
            context.reported_problems()
        );
        assert_eq!(result.as_delete().unwrap().result_columns().len(), width);
        if sql.contains(".*") {
            let column = result
                .as_delete()
                .unwrap()
                .result_columns()
                .get_column_at(0)
                .unwrap();
            let binding = column.expression().unwrap().delete_row_reference().unwrap();
            assert!(binding.source().get_column_def().is_some());
            assert_eq!(
                binding.version(),
                if sql.contains("old.*") {
                    crate::sem::DeleteRowVersion::old()
                } else {
                    crate::sem::DeleteRowVersion::new_image()
                }
            );
        }
    }
}

impl DeleteRowColumn {
    /// Row image selected by the reference.
    pub const fn version(&self) -> DeleteRowVersion {
        self.version
    }
    /// Completed target-column lookup, including a missing-column outcome.
    pub fn source(&self) -> &ColumnReferenceExpr {
        &self.source
    }
    pub(crate) fn column_type(&self) -> Option<TypeReference> {
        self.source.get_type()
    }
    pub(crate) fn is_not_null(&self) -> Option<bool> {
        if self.version.is_new() {
            Some(false)
        } else {
            self.source.is_not_null()
        }
    }
}

#[cfg(test)]
mod test_delete_row_images_keep_the_destination_occurrence {

    use crate::{
        names::FullName,
        sem::{PgBuiltInType, SemScalarExpr},
        test_helpers::query::source_namespace_statement_at_version,
    };

    #[rstest::rstest]
    #[case::old("old", true)]
    #[case::new("new", false)]
    fn delete_row_images_keep_the_destination_occurrence(
        #[case] image: &str,
        #[case] not_null: bool,
    ) {
        let sql = format!("DELETE FROM a.t USING b.t RETURNING {image}.*");
        let analyzed = source_namespace_statement_at_version(&sql, "18");
        assert!(
            analyzed.context().reported_problems().is_empty(),
            "{:?}",
            analyzed.context().reported_problems()
        );
        let columns = analyzed.ast().as_delete().unwrap().result_columns();
        assert_eq!(columns.len(), 2);
        for index in 0..columns.len() {
            let column = columns.get_column_at(index).unwrap();
            assert_eq!(column.col_type(), &Some(PgBuiltInType::int4()));
            assert_eq!(column.is_not_null(), &Some(not_null));
            let expression = column.expression().unwrap();
            let SemScalarExpr::ColumnRef(reference) = expression else {
                panic!("row-image column expected");
            };
            let row = reference.delete_row_reference().unwrap();
            assert_eq!(
                row.source().table_column_reference().unwrap().relation(),
                &FullName::with_schema("a", "t")
            );
        }
    }
}

impl DeleteRowColumn {
    pub(crate) fn column_definition(&self) -> &Option<ColumnDefinition> {
        self.source.get_column_def()
    }
    pub(crate) fn column_name(&self) -> &ColumnName {
        self.source.get_column_name()
    }
}
