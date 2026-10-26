use crate::{
    StringSpan,
    sem::{AnalysisError, SourcePathWithSpan},
};
use ::sql_fun_core::SqlFunArgs;
use std::{
    io::{Read, Seek},
    path::Path,
};

/// provide access to source SQL file
pub trait SourceAccessService: std::fmt::Debug {
    /// Get source span as String
    fn get_source_span(
        &self,
        _config_args: &SqlFunArgs,
        source_path: &Path,
        span: StringSpan,
    ) -> Result<String, AnalysisError> {
        let source_path = if source_path.is_absolute() {
            source_path
        } else {
            // TODO resolve path
            source_path
        };

        if !source_path.exists() {
            AnalysisError::file_not_found(source_path)?;
            AnalysisError::never()
        };

        let mut file = std::fs::File::open(source_path)?;
        file.seek(span.seek_pos())?;
        let mut source_span = file.take(span.len() as u64);
        let mut buf = String::default();
        let result_len = source_span.read_to_string(&mut buf)?;
        if result_len != span.len() {
            Err(AnalysisError::unexpected_eof())
        } else {
            Ok(buf)
        }
    }
}

/// default implementation for SourceAccessService
#[derive(Debug, Default)]
pub struct DefaultSourceAccessService {}

impl SourceAccessService for DefaultSourceAccessService {}

/// can get source path with span
pub trait GetSourcePathWithSpan {
    /// get source path with span
    fn get_source_path_with_span(&self) -> &SourcePathWithSpan;

    /// get source path
    fn get_source_path(&self) -> &Path {
        self.get_source_path_with_span().source_path()
    }

    /// get source span
    fn get_span(&self) -> &StringSpan {
        self.get_source_path_with_span().span()
    }
}

/// support mutate source path with span
pub trait MutSourcePathWithSpan {
    /// set source path with span
    fn set_source_path_with_span(&mut self, value: &SourcePathWithSpan);
}

/// cant get optional source path with span
pub trait GetSourcePathWithSpanOpt {
    /// get source path with span
    fn get_source_path_with_span(&self) -> Option<&SourcePathWithSpan>;

    /// get source path
    fn get_source_path(&self) -> Option<&Path> {
        self.get_source_path_with_span().map(|v| v.source_path())
    }

    /// get source span
    fn get_span(&self) -> Option<&StringSpan> {
        self.get_source_path_with_span().map(|v| v.span())
    }
}
