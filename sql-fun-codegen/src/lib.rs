#![deny(missing_docs)]

//!
//! `sql-fun-codegen`: output / input for Rust AST
//!

use proc_macro2::{Span, TokenStream};
use syn::{LitStr, parse::Parse, visit::Visit};

/// Error related Syn IO
#[derive(Debug, thiserror::Error)]
pub enum SqlFunCodegenError {
    /// represents [`syn::Error`]
    #[error(transparent)]
    SynError(#[from] syn::Error),
}

/// common sql-fun Attribute argument AST
#[derive(Debug)]
pub struct SqlFunOptions {
    /// SQL statement literal
    sql: Option<String>,

    /// SQL statement from file
    sql_file: Option<String>,

    /// data source name
    datasource: Option<String>,
}

impl Parse for SqlFunOptions {
    fn parse(input: syn::parse::ParseStream<'_>) -> syn::Result<Self> {
        let mut options = Self {
            sql: None,
            sql_file: None,
            datasource: None,
        };

        let arguments = input.parse_terminated(syn::MetaNameValue::parse, syn::Token![,])?;
        for argument in arguments {
            let value = match argument.value {
                syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(value),
                    ..
                }) => value.value(),
                expression => {
                    return Err(syn::Error::new_spanned(
                        expression,
                        "expected string literal",
                    ));
                }
            };

            if argument.path.is_ident("sql") {
                options.sql = Some(value);
            } else if argument.path.is_ident("sql_file") {
                options.sql_file = Some(value);
            } else if argument.path.is_ident("datasource") {
                options.datasource = Some(value);
            } else {
                return Err(syn::Error::new_spanned(argument.path, "unknown field"));
            }
        }

        Ok(options)
    }
}

impl SqlFunOptions {
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
        #[case] sql: &str,
        #[case] expect_sql: Option<&str>,
        #[case] expect_sql_file: Option<&str>,
        #[case] datasource: Option<&str>,
    ) -> testresult::TestResult {
        let item_fn: ItemFn = syn::parse_str(sql)?;
        let sql_fun_options = item_fn.attrs[0].parse_args::<super::SqlFunOptions>()?;
        assert_eq!(sql_fun_options.sql(), expect_sql);
        assert_eq!(sql_fun_options.sql_file(), expect_sql_file);
        assert_eq!(sql_fun_options.datasource(), datasource);
        Ok(())
    }
}

/// CTE reference
#[derive(Debug)]
pub struct CteReference {
    cte_name: String,
    alias: String,
}

impl Parse for CteReference {
    fn parse(input: syn::parse::ParseStream<'_>) -> syn::Result<Self> {
        let mut cte_name = None;
        let mut alias = None;

        let arguments = input.parse_terminated(syn::MetaNameValue::parse, syn::Token![,])?;
        for argument in arguments {
            let value = match argument.value {
                syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(value),
                    ..
                }) => value.value(),
                expression => {
                    return Err(syn::Error::new_spanned(
                        expression,
                        "expected string literal",
                    ));
                }
            };

            if argument.path.is_ident("cte_name") {
                cte_name = Some(value);
            } else if argument.path.is_ident("alias") {
                alias = Some(value);
            } else {
                return Err(syn::Error::new_spanned(argument.path, "unknown field"));
            }
        }

        Ok(Self {
            cte_name: cte_name.ok_or_else(|| input.error("missing field `cte_name`"))?,
            alias: alias.ok_or_else(|| input.error("missing field `alias`"))?,
        })
    }
}

impl CteReference {
    /// name of catalog item
    #[must_use]
    pub const fn cte_name(&self) -> &str {
        self.cte_name.as_str()
    }

    /// alias name for query
    #[must_use]
    pub const fn alias(&self) -> &str {
        self.alias.as_str()
    }

    /// gets [`CteReference`] from [`syn::Attribute`]
    ///
    /// # Errors
    ///
    /// - [`syn::Error`]: invalid syntax
    ///
    pub fn from_attr(attr: &syn::Attribute) -> Result<Option<Self>, syn::Error> {
        if attr.meta.path().is_ident("with") {
            Ok(Some(attr.parse_args()?))
        } else {
            Ok(None)
        }
    }

    /// gets [`Vec<CteReference>`] from [`syn::ItemFn`]
    ///
    /// # Errors
    ///
    /// - [`syn::Error`]: invalid syntax
    ///
    pub fn from_item_fn(item_fn: &syn::ItemFn) -> Result<Vec<Self>, syn::Error> {
        let mut results = Vec::new();
        for attr in &item_fn.attrs {
            if let Some(att) = Self::from_attr(attr)? {
                results.push(att);
            }
        }
        Ok(results)
    }
}

#[cfg(test)]
mod test_cte_reference {

    #[rstest::rstest]
    #[case(
        r#"
            #[sql_fun::sql_query_one(
                sql="select id, name from au where name=${id}"
            )]
            #[with(cte_name="active_users", alias="au")]
            pub async fn select_user_by_id(client: Client, id: u64) -> Result<User, tokio_postgres::Error> { }
        "#,
        "active_users",
        "au"
    )]
    fn test_parse_cte_reference(
        #[case] attr: &str,
        #[case] cte_name: &str,
        #[case] alias: &str,
    ) -> testresult::TestResult {
        let item: syn::ItemFn = syn::parse_str(attr)?;
        let cte_reference = super::CteReference::from_item_fn(&item)?;
        assert_eq!(cte_reference[0].cte_name(), cte_name);
        assert_eq!(cte_reference[0].alias(), alias);
        Ok(())
    }
}

/// Source embeddable SQL Statement string
#[derive(Debug)]
pub struct SQLStatement {
    span: Span,
    sql_text: String,
    datasource: String,
}

impl SQLStatement {
    /// create instance
    #[must_use]
    pub fn new(sql_text: &str, span: &Span, datasource: &str) -> Self {
        Self {
            span: *span,
            sql_text: String::from(sql_text),
            datasource: String::from(datasource),
        }
    }

    /// convert to [`TokenStream`]
    #[must_use]
    pub fn into_quote(self) -> TokenStream {
        let sql_text = LitStr::new(&self.sql_text, self.span);
        let source_path = LitStr::new(&self.span.file(), self.span);
        let datasource = LitStr::new(&self.datasource, self.span);

        quote::quote! {
            #[expect(dead_code)]
            fn __sql_fun_query_info() {
                const SQL_TEXT: &str = #sql_text;
                const SOURCE: &str = #source_path;
                const DATASOURCE: &str = #datasource;
            }
        }
    }

    const QUERY_INFO_FUN_NAME: &str = "__sql_fun_query_info";
}

/// Source embedded SQL Statement
#[derive(Debug)]
pub struct EncodedSQLStatement {
    sql_text: LitStr,
    file: LitStr,
    datasource: LitStr,
}

impl EncodedSQLStatement {
    /// SQL text literal
    #[must_use]
    pub const fn sql_text(&self) -> &LitStr {
        &self.sql_text
    }

    /// embedded location file
    #[must_use]
    pub const fn file(&self) -> &LitStr {
        &self.file
    }

    /// specified datasource
    #[must_use]
    pub const fn datasource(&self) -> &LitStr {
        &self.datasource
    }
}

/// collect `EncodedSQLStatement` from `syn::File`
#[derive(Debug, Default)]
pub struct SQLQueryInfoVisitor {
    last_lit_str: Option<LitStr>,
    sql_text: Option<LitStr>,
    file: Option<LitStr>,
    datasource: Option<LitStr>,
    results: Vec<EncodedSQLStatement>,
}

impl SQLQueryInfoVisitor {
    /// get SQL statements embedded in source code
    #[must_use]
    pub fn sql_statements(file: &syn::File) -> Vec<EncodedSQLStatement> {
        let mut visitor = Self::default();
        visitor.visit_file(file);
        visitor.results
    }
}

impl<'ast> Visit<'ast> for SQLQueryInfoVisitor {
    fn visit_item_fn(&mut self, i: &'ast syn::ItemFn) {
        if i.sig.ident == SQLStatement::QUERY_INFO_FUN_NAME {
            self.visit_block(&i.block);
            if let Some(sql_text) = self.sql_text.clone()
                && let Some(file) = self.file.clone()
                && let Some(datasource) = self.datasource.clone()
            {
                self.sql_text = None;
                self.file = None;
                self.results.push(EncodedSQLStatement {
                    sql_text,
                    file,
                    datasource,
                });
            }
        } else {
            syn::visit::visit_item_fn(self, i);
        }
    }

    fn visit_item_const(&mut self, i: &'ast syn::ItemConst) {
        if i.ident == "SQL_TEXT" {
            self.visit_expr(&i.expr);
            self.sql_text = self.last_lit_str.clone();
        }

        if i.ident == "SOURCE" {
            self.visit_expr(&i.expr);
            self.file = self.last_lit_str.clone();
        }

        if i.ident == "DATASOUCE" {
            self.visit_expr(&i.expr);
            self.datasource = self.last_lit_str.clone();
        }
    }

    fn visit_lit_str(&mut self, i: &'ast syn::LitStr) {
        self.last_lit_str = Some(i.clone());
    }
}
