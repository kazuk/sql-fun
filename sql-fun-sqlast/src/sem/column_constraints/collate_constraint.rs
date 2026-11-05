#[cfg(feature = "sqlite")]
use crate::{SqliteToken, syn::sqlite::CollateColumnConstraint as SqliteCollateColumnConstraint};
use crate::{
    names::{ColumnName, TableName},
    sem::{
        AnalysisError, AnalysisProblem, ColumnConstraint, ColumnConstraintKind,
        ColumnConstraintTrait, ColumnRename, ParseContext,
    },
};
#[cfg(feature = "sqlite")]
use sql_fun_core::IVec;

/// collate constraint
#[derive(Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize, Clone)]
pub struct CollateColumnConstraint {
    table_name: TableName,
    column_name: ColumnName,
    collation_name: String,
}

impl CollateColumnConstraint {
    pub(crate) fn rename_column(&mut self, rename: &ColumnRename, owned: bool) {
        if owned {
            rename.rename_name(&mut self.column_name);
        }
    }

    pub(crate) fn rename_relation(&mut self, old: &TableName, new: &TableName) {
        self.table_name.rename_reference(old, new);
    }

    pub(crate) fn is_known_name(name: &str) -> bool {
        ["BINARY", "NOCASE", "RTRIM"].contains(&name.to_uppercase().as_str())
    }
    /// create instance
    pub fn new(table_name: &TableName, column_name: &ColumnName, collation_name: &str) -> Self {
        Self {
            table_name: table_name.clone(),
            column_name: column_name.clone(),
            collation_name: String::from(collation_name),
        }
    }

    /// get collation name
    pub fn collation_name(&self) -> &str {
        &self.collation_name
    }
}

impl ColumnConstraintTrait for CollateColumnConstraint {
    fn kind(&self) -> ColumnConstraintKind {
        ColumnConstraintKind::Collate
    }

    fn table_name(&self) -> &TableName {
        &self.table_name
    }

    fn column_name(&self) -> &ColumnName {
        &self.column_name
    }
}

#[cfg(feature = "sqlite")]
impl CollateColumnConstraint {
    /// analyze [`crate::syn::sqlite::CollateColumnConstraint`]
    pub fn analyze_constraint_sqlite<TParseContext>(
        mut context: TParseContext,
        table_name: &TableName,
        column_name: &ColumnName,
        syn: &SqliteCollateColumnConstraint,
        _tokens: &IVec<SqliteToken<'_>>,
    ) -> Result<(ColumnConstraint, TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        let collation_name = syn.collation_name().value();
        if !Self::is_known_name(collation_name) {
            context.report_analysis_problem(AnalysisProblem::collation_name_not_found(
                collation_name,
            ))?;
        }
        Ok((
            ColumnConstraint::collate(Self::new(table_name, column_name, collation_name)),
            context,
        ))
    }
}

impl ColumnConstraint {
    /// create Collate variant
    pub const fn collate(value: CollateColumnConstraint) -> Self {
        Self::Collate(value)
    }

    /// get Collate variant
    pub const fn as_collate(&self) -> Option<&CollateColumnConstraint> {
        if let Self::Collate(c) = self {
            Some(c)
        } else {
            None
        }
    }
}

// Analysis owned by the semantic definitions in this module.

impl CollateColumnConstraint {
    pub(in crate::sem::column_constraints) fn from_ast_name(
        context: &mut impl ParseContext,
        table: &TableName,
        column: &ColumnName,
        name: &str,
    ) -> Result<Self, AnalysisError> {
        if !Self::is_known_name(name) {
            context.report_analysis_problem(AnalysisProblem::collation_name_not_found(name))?;
        }
        Ok(Self::new(table, column, name))
    }
}

#[cfg(all(test, feature = "sqlite"))]
#[cfg(test)]
mod test_analyze_sqlite_column_definition_reports_unknown_collation {
    use crate::test_helpers::column_definition::parse_create_table;

    use crate::{
        names::TableName,
        sem::{ColumnConstraint, ColumnDefinition},
        syn::sqlite::CreateTableBody,
        test_helpers::column_definition::setup_context,
    };

    #[test]
    fn analyze_sqlite_column_definition_reports_unknown_collation() {
        let sql = "create table users (name text collate custom)";
        let (create_table, tokens) = parse_create_table(sql);
        let body = match create_table.body() {
            CreateTableBody::ColumnsAndConstraint(body) => body,
            _ => panic!("expected column-based create table body"),
        };
        let table_name = TableName::try_from(create_table.table_name())
            .expect("table name should convert into semantic table name");

        let (column_def, context) = ColumnDefinition::analyze_column_definition(
            setup_context(),
            &table_name,
            &body.columns()[0],
            &tokens,
        )
        .expect("column definition should analyze");

        assert_eq!(column_def.constraints().len(), 1);
        match &column_def.constraints()[0] {
            ColumnConstraint::Collate(collate) => {
                assert_eq!(collate.collation_name(), "custom")
            }
            _ => panic!("expected collate constraint"),
        }
        assert_eq!(context.reported_problem_count(), 1);
    }
}
