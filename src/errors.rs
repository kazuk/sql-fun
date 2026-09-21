use proc_macro::TokenStream;

#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct SqlFunError(SqlFunErrorInner);

#[derive(Debug, thiserror::Error)]
pub enum SqlFunErrorInner {
    #[error("Syntax error: {0}")]
    Syntax(#[from] syn::Error),

    #[error("Metadata error: {0}")]
    Metadata(#[from] toml::de::Error),

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
            Self::Syntax(err) => err.clone().into_compile_error().into(),
            Self::Metadata(err) => syn::Error::new_spanned("", format!("Metadata error: {err:?}"))
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
}

impl From<syn::Error> for SqlFunError {
    fn from(err: syn::Error) -> Self {
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
