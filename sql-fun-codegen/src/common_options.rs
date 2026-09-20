use std::path::Path;

use serde_json_path::JsonPath;
use sql_fun_core::PlaceholderedSql;

use crate::{JsonValueCache, SqlFunCodegenError};

/// common sql-fun Attribute argument AST
#[derive(Debug, Default)]
pub struct SqlFunOptions {
    /// SQL statement literal
    sql: Option<String>,

    /// SQL statement from file
    sql_file: Option<String>,

    /// data source name
    datasource: Option<String>,
}

impl SqlFunOptions {
    pub(crate) fn set_name_value(&mut self, value: &syn::MetaNameValue) -> bool {
        let syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Str(literal),
            ..
        }) = &value.value
        else {
            return false;
        };
        let literal_value = literal.value();
        if value.path.is_ident("sql") {
            self.sql = Some(literal_value);
        } else if value.path.is_ident("sql_file") {
            self.sql_file = Some(literal_value);
        } else if value.path.is_ident("datasource") {
            self.datasource = Some(literal_value);
        } else {
            return false;
        }
        true
    }

    /// Source code embedded literal sql
    #[must_use]
    pub fn sql(&self) -> Option<&str> {
        self.sql.as_deref()
    }

    /// SQL file
    #[must_use]
    pub fn sql_file(&self) -> Option<&str> {
        self.sql_file.as_deref()
    }

    /// `datasource` argument value
    #[must_use]
    pub fn datasource(&self) -> Option<&str> {
        self.datasource.as_deref()
    }

    /// get specified query text
    pub fn query_text(&self) -> Result<String, SqlFunCodegenError> {
        if let Some(sql) = self.sql() {
            Ok(String::from(sql))
        } else {
            let Some(sql_file) = self.sql_file() else {
                return SqlFunCodegenError::require_sql_or_sql_file();
            };
            if let Some((file_path, item_path)) = sql_file.split_once('#') {
                let json_path = JsonPath::parse(item_path)?;

                let path = Path::new(file_path);
                if !path.exists() {
                    return SqlFunCodegenError::file_not_found(file_path);
                }
                let query = JsonValueCache::get_or_fill(path, |value| {
                    let node = json_path.query(value).exactly_one()?;
                    let Some(query) = node.as_str() else {
                        return SqlFunCodegenError::require_string_value(file_path, item_path);
                    };
                    Ok(String::from(query))
                })?;
                Ok(query)
            } else {
                let sql_file = Path::new(sql_file);
                if !sql_file.exists() {
                    return SqlFunCodegenError::file_not_found(sql_file);
                }
                Ok(std::fs::read_to_string(sql_file)?)
            }
        }
    }

    pub fn query(&self) -> Result<PlaceholderedSql, SqlFunCodegenError> {
        let query = self.query_text()?;
        Ok(PlaceholderedSql::from_query(&query))
    }
}

#[cfg(test)]
mod test_sql_fun_options {
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
        None,
        None
    )]
    #[case(
        r#"
#[sql_fun::sql_query_one(
    sql_file="select_user_by_id.sql",
    datasource="default"
)]
pub async fn select_user_by_id(client: Client, id: u64) -> Result<User, tokio_postgres::Error> { }
"#,
        None,
        Some("select_user_by_id.sql"),
        Some("default")
    )]
    fn test_parse_sql_fun_options_sql(
        #[case] rust_source: &str,
        #[case] expect_sql: Option<&str>,
        #[case] expect_sql_file: Option<&str>,
        #[case] datasource: Option<&str>,
    ) -> testresult::TestResult {
        let item_fn: ItemFn = syn::parse_str(rust_source)?;
        let attr = &item_fn.attrs[0];
        let sql_fun_options = attr.parse_args::<crate::SqlQueryOptions>()?;
        let sql_fun_options = sql_fun_options.common_options;
        assert_eq!(sql_fun_options.sql(), expect_sql);
        assert_eq!(sql_fun_options.sql_file(), expect_sql_file);
        assert_eq!(sql_fun_options.datasource(), datasource);
        Ok(())
    }
}
