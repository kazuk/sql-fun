use crate::{
    ast::{Deferrability, ForeignKeyAction, ForeignKeyMatchType, Validity},
    names::{ColumnName, TableName},
    sem::{
        ColumnConstraint, ColumnConstraintKind, ColumnConstraintTrait, ColumnRename,
        ForeignKeyTarget, TypeReference,
    },
};

/// foreign key constraint
#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub struct ForeignKeyColumnConstraint {
    table_name: TableName,
    column_name: ColumnName,
    primary_key_table: ForeignKeyTarget,
    primary_key_column: ColumnName,
    primary_key_column_type: Option<TypeReference>,
    match_type: Option<ForeignKeyMatchType>,
    delete_action: Option<ForeignKeyAction>,
    update_action: Option<ForeignKeyAction>,
    deferrablity: Option<Deferrability>,
    validity: Option<Validity>,
}

impl ForeignKeyColumnConstraint {
    pub(crate) fn rename_column(&mut self, rename: &ColumnRename, owned: bool) {
        if owned {
            rename.rename_name(&mut self.column_name);
        }
        if self
            .primary_key_table
            .relation()
            .is_some_and(|table| rename.matches_table(table))
        {
            rename.rename_name(&mut self.primary_key_column);
        }
    }

    pub(crate) fn rename_relation(&mut self, old: &TableName, new: &TableName) {
        self.table_name.rename_reference(old, new);
        self.primary_key_table.rename_relation(old, new);
    }

    /// Create a constraint with a canonical target, or `None` for unresolved lookup.
    ///
    /// The caller supplies a completed lookup result, never an unqualified SQL name.
    #[expect(clippy::too_many_arguments)]
    pub fn new(
        table_name: &TableName,
        column_name: &ColumnName,
        primary_key_table: Option<&TableName>,
        primary_key_column: &ColumnName,
        primary_key_column_type: Option<&TypeReference>,
        match_type: Option<&ForeignKeyMatchType>,
        delete_action: Option<ForeignKeyAction>,
        update_action: Option<ForeignKeyAction>,
        deferrablity: Option<Deferrability>,
        validity: Option<Validity>,
    ) -> Self {
        Self {
            table_name: table_name.clone(),
            column_name: column_name.clone(),
            primary_key_table: ForeignKeyTarget::new(primary_key_table),
            primary_key_column: primary_key_column.clone(),
            primary_key_column_type: primary_key_column_type.cloned(),
            match_type: match_type.cloned(),
            delete_action,
            update_action,
            deferrablity,
            validity,
        }
    }
}

#[cfg(test)]
mod test_foreign_key_serialization {
    use crate::{
        names::{ColumnName, FullName},
        sem::{ForeignKeyColumnConstraint, PgBuiltInType},
        test_helpers::foreign_keys as fk,
    };

    #[test]
    fn inline_type_and_current_serialization_preserve_completed_outcomes() -> testresult::TestResult
    {
        let context = fk::bind(
            fk::parents(),
            "child",
            "parent(id) ON DELETE CASCADE NOT DEFERRABLE",
            false,
        );
        let constraint = fk::inline(&context, &FullName::with_schema("a", "child"));
        assert_eq!(
            constraint.primary_key_column_type(),
            Some(&PgBuiltInType::int4())
        );
        assert_eq!(constraint.primary_key_column(), &ColumnName::from("id"));
        let json = ::serde_json::to_value(&constraint)?;
        assert_eq!(json["delete_action"], "Cascade");
        assert_eq!(json["deferrablity"]["deferrable"], false);
        let restored: ForeignKeyColumnConstraint = ::serde_json::from_value(json)?;
        assert_eq!(restored, constraint);
        let context = fk::bind(context, "unresolved", "missing(id)", false);
        let unresolved = fk::inline(&context, &FullName::with_schema("a", "unresolved"));
        assert!(unresolved.primary_key_table().is_none());
        let json = ::serde_json::to_value(&unresolved)?;
        assert!(json["primary_key_table"].is_null());
        let restored: ForeignKeyColumnConstraint = ::serde_json::from_value(json)?;
        assert_eq!(restored, unresolved);
        Ok(())
    }
}

impl ForeignKeyColumnConstraint {
    /// Canonical referenced relation, or `None` after failed target lookup.
    ///
    /// Later search-path changes do not alter this retained binding.
    pub const fn primary_key_table(&self) -> Option<&TableName> {
        self.primary_key_table.relation()
    }

    /// get primary key column name
    pub const fn primary_key_column(&self) -> &ColumnName {
        &self.primary_key_column
    }

    /// get primary key column type
    pub const fn primary_key_column_type(&self) -> Option<&TypeReference> {
        self.primary_key_column_type.as_ref()
    }

    /// get match type
    pub const fn match_type(&self) -> Option<&ForeignKeyMatchType> {
        self.match_type.as_ref()
    }
}

impl ColumnConstraintTrait for ForeignKeyColumnConstraint {
    fn kind(&self) -> ColumnConstraintKind {
        ColumnConstraintKind::ForeignKey
    }

    fn table_name(&self) -> &TableName {
        &self.table_name
    }

    fn column_name(&self) -> &ColumnName {
        &self.column_name
    }
}

impl ColumnConstraint {
    /// create ForeignKey variant
    pub const fn foreign_key(value: ForeignKeyColumnConstraint) -> Self {
        Self::ForeignKey(value)
    }

    /// get ForeignKey variant
    pub const fn as_foreign_key(&self) -> Option<&ForeignKeyColumnConstraint> {
        if let Self::ForeignKey(c) = self {
            Some(c)
        } else {
            None
        }
    }
}
