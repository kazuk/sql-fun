use crate::{
    engine_specific::TursoOptimizeIndexFeature,
    sem::AnalysisError,
    syn::sqlite::{Expr, Name, QualifiedName, SortedColumn},
};

/// create index statement
#[derive(derive_getters::Getters, Debug)]
pub struct CreateIndexStatement {
    unique: bool,
    if_not_exists: bool,
    idx_name: QualifiedName,
    table_name: Name,
    using: Option<Name>,
    columns: Vec<SortedColumn>,
    with_clause: Vec<(Name, Box<Expr>)>,
    where_clause: Option<Box<Expr>>,
}

impl CreateIndexStatement {
    /// create instance
    #[expect(clippy::too_many_arguments)]
    pub fn new(
        unique: bool,
        if_not_exists: bool,
        idx_name: QualifiedName,
        table_name: Name,
        using: Option<Name>,
        columns: Vec<SortedColumn>,
        with_clause: Vec<(Name, Expr)>,
        where_clause: Option<Expr>,
    ) -> Self {
        Self {
            unique,
            if_not_exists,
            idx_name,
            table_name,
            using,
            columns,
            with_clause: with_clause
                .into_iter()
                .map(|v| (v.0, Box::new(v.1)))
                .collect(),
            where_clause: where_clause.map(Box::new),
        }
    }
}

/// drop index statement
#[derive(derive_getters::Getters, Debug)]
pub struct DropIndexStatement {
    if_exists: bool,
    idx_name: QualifiedName,
}

impl DropIndexStatement {
    /// create instance
    pub const fn new(if_exists: bool, idx_name: QualifiedName) -> Self {
        Self {
            if_exists,
            idx_name,
        }
    }
}

/// reindex statement
#[derive(derive_getters::Getters, Debug)]
pub struct ReindexStatement {
    idx_name: Option<QualifiedName>,
}

impl ReindexStatement {
    /// create instance
    pub const fn new(idx_name: Option<QualifiedName>) -> Self {
        Self { idx_name }
    }
}

/// optimize statement
#[derive(Debug)]
pub struct OptimizeStatement {
    idx_name: Option<QualifiedName>,
}

impl OptimizeStatement {
    /// create instance
    pub const fn new(idx_name: Option<QualifiedName>) -> Self {
        Self { idx_name }
    }

    /// index name
    pub fn idx_name<Gate>(&self, gate: &mut Gate) -> Result<Option<&QualifiedName>, AnalysisError>
    where
        Gate: TursoOptimizeIndexFeature,
    {
        gate.optimize_statement_used(self)?;
        Ok(self.idx_name.as_ref())
    }
}

#[cfg(test)]
mod test_index_names {
    use crate::syn::sqlite::Stmt;
    use crate::test_helpers::SqliteSynBuilder;

    #[rstest::rstest]
    #[case::create("CREATE INDEX aux.idx ON items(id)")]
    #[case::drop("DROP INDEX aux.idx")]
    #[case::reindex("REINDEX aux.idx")]
    fn qualified_index_syntax_preserves_database(#[case] sql: &str) {
        let statement = SqliteSynBuilder.parse_as_stmt(sql);
        let name = match &statement {
            Stmt::CreateIndex(index) => {
                assert_eq!(index.table_name().value(), "items");
                index.idx_name()
            }
            Stmt::DropIndex(index) => index.idx_name(),
            Stmt::Reindex(index) => index.idx_name().as_ref().expect("index name"),
            _ => panic!("expected index statement"),
        };
        assert_eq!(name.name().value(), "idx");
        assert_eq!(name.db_name().expect("database").value(), "aux");
    }

    #[test]
    fn sqlite_create_rejects_schema_qualification_on_table() {
        assert!(crate::SqliteDialect::parse("CREATE INDEX idx ON aux.items(id);").is_err());
    }
}
