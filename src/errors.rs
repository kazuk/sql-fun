use proc_macro::TokenStream;

use crate::TokioPostgresCodeGeneratorBuilderError;

#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct SqlFunError(SqlFunErrorInner);

#[derive(Debug, thiserror::Error)]
pub enum SqlFunErrorInner {
    #[error("SQL parse error: {0}")]
    SqlParse(#[from] pg_query::Error),

    #[error("Syntax error: {0}")]
    Syntax(#[from] syn::Error),

    #[error("Metadata error: {0}")]
    Metadata(#[from] toml::de::Error),

    #[error("{0}")]
    Custom(String),

    #[error("Unnamed column found in result set.")]
    UnnamedColumnInResultSet,

    #[error("Code generator setup failed {0}")]
    GeneratorBuilder(#[from] TokioPostgresCodeGeneratorBuilderError),

    #[error("file IO error")]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Args(#[from] sql_fun_core::SqlFunArgsError),

    #[error(transparent)]
    ArgMetadata(#[from] sql_fun_core::MetadataError),
}

impl SqlFunErrorInner {
    pub fn to_compile_error(&self) -> TokenStream {
        match self {
            Self::GeneratorBuilder(err) => syn::Error::new_spanned(
                "",
                format!("unexpected code generator setup failed {err:?}"),
            )
            .into_compile_error()
            .into(),
            Self::SqlParse(err) => syn::Error::new_spanned("", format!("SQL parse error: {err:?}"))
                .into_compile_error()
                .into(),
            Self::Syntax(err) => err.clone().into_compile_error().into(),
            Self::Metadata(err) => syn::Error::new_spanned("", format!("Metadata error: {err:?}"))
                .into_compile_error()
                .into(),
            Self::Custom(err) => syn::Error::new_spanned("", err).into_compile_error().into(),
            Self::UnnamedColumnInResultSet => syn::Error::new_spanned(
                "",
                r#"Unnamed column found in result set.
Column expressions must have an alias.
Fix: Use `AS` to name computed columns."#,
            )
            .into_compile_error()
            .into(),
            Self::Io(e) => syn::Error::new_spanned("", format!("IO error {e:?}"))
                .into_compile_error()
                .into(),
            Self::Args(e) => syn::Error::new_spanned("", format!("Argument error {e:?}"))
                .into_compile_error()
                .into(),
            Self::ArgMetadata(e) => syn::Error::new_spanned("", format!("Metadata error {e:?}"))
                .into_compile_error()
                .into(),
        }
    }
}

impl SqlFunError {
    pub fn to_compile_error(&self) -> TokenStream {
        self.0.to_compile_error()
    }

    pub fn custom(message: &str) -> Self {
        Self(SqlFunErrorInner::Custom(message.to_string()))
    }

    pub fn unnamed_column_in_result_rowset() -> Self {
        Self(SqlFunErrorInner::UnnamedColumnInResultSet)
    }
}

impl From<pg_query::Error> for SqlFunError {
    fn from(err: pg_query::Error) -> Self {
        Self(SqlFunErrorInner::from(err))
    }
}

impl From<syn::Error> for SqlFunError {
    fn from(err: syn::Error) -> Self {
        Self(SqlFunErrorInner::from(err))
    }
}

impl From<TokioPostgresCodeGeneratorBuilderError> for SqlFunError {
    fn from(err: TokioPostgresCodeGeneratorBuilderError) -> Self {
        Self(SqlFunErrorInner::from(err))
    }
}

impl From<std::io::Error> for SqlFunError {
    fn from(err: std::io::Error) -> Self {
        Self(SqlFunErrorInner::from(err))
    }
}

impl From<sql_fun_core::SqlFunArgsError> for SqlFunError {
    fn from(err: sql_fun_core::SqlFunArgsError) -> Self {
        Self(SqlFunErrorInner::from(err))
    }
}

impl From<sql_fun_core::MetadataError> for SqlFunError {
    fn from(err: sql_fun_core::MetadataError) -> Self {
        Self(SqlFunErrorInner::from(err))
    }
}
