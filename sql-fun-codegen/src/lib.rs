#[derive(Debug, thiserror::Error)]
pub enum SqlFunCodegenError {
    #[error(transparent)]
    SynError(#[from] syn::Error),
}

#[derive(Debug, darling::FromMeta)]
#[darling(derive_syn_parse)]
pub struct SqlFunOptions {
    /// SQL statement literal
    sql: Option<String>,

    /// SQL statement from file
    sql_file: Option<String>,

    /// data source name
    datasource: Option<String>,
}

impl SqlFunOptions {
    #[must_use]
    pub fn sql(&self) -> Option<&str> {
        self.sql.as_deref()
    }

    #[must_use]
    pub fn sql_file(&self) -> Option<&str> {
        self.sql_file.as_deref()
    }

    #[must_use]
    pub fn datasource(&self) -> Option<&str> {
        self.datasource.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use darling::FromMeta;
    use syn::ItemFn;

    #[rstest::rstest]
    #[case(
        r#"
#[sql_fun::sql_query_one(
    sql="select id, name from users where name=${id}"
)]
pub async fn select_user_by_id(client: Client, id: u64) -> Result<User, tokio_postgres::Error> { }
"#,
        Some("select id, name from users where name=${id}"),
        None
    )]
    #[case(
        r#"
#[sql_fun::sql_query_one(
    sql_file="select_user_by_id.sql"
)]
pub async fn select_user_by_id(client: Client, id: u64) -> Result<User, tokio_postgres::Error> { }
"#,
        None,
        Some("select_user_by_id.sql")
    )]
    fn test_parse_sql_fun_options_sql(
        #[case] sql: &str,
        #[case] expect_sql: Option<&str>,
        #[case] expect_sql_file: Option<&str>,
    ) -> testresult::TestResult {
        let item_fn: ItemFn = syn::parse_str(sql)?;
        let sql_fun_options = super::SqlFunOptions::from_meta(&item_fn.attrs[0].meta)?;
        assert_eq!(sql_fun_options.sql(), expect_sql);
        assert_eq!(sql_fun_options.sql_file(), expect_sql_file);
        Ok(())
    }
}
