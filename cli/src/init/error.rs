#[derive(Debug, thiserror::Error)]
pub enum InitError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Prompt(#[from] inquire::InquireError),
    #[error("{}", Self::unexpected_version_message(.0))]
    UnexpectedVersion(String),
}

impl InitError {
    pub const fn io(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

#[cfg(test)]
mod test_io {
    use super::InitError;
    use std::error::Error as StdError;
    use std::io::ErrorKind;

    #[rstest::rstest]
    #[case::permission_denied(ErrorKind::PermissionDenied)]
    #[case::not_a_directory(ErrorKind::NotADirectory)]
    fn preserves_io_source_and_message(#[case] kind: ErrorKind) {
        let error = InitError::io(std::io::Error::new(kind, "create failed"));
        assert_eq!(error.to_string(), "create failed");
        let source = error
            .source()
            .unwrap()
            .downcast_ref::<std::io::Error>()
            .unwrap();
        assert_eq!(source.kind(), kind);
    }
}

impl InitError {
    pub const fn prompt(error: inquire::InquireError) -> Self {
        Self::Prompt(error)
    }
}

#[cfg(test)]
mod test_prompt {
    use super::InitError;
    use std::error::Error as StdError;

    #[rstest::rstest]
    #[case::cancelled(inquire::InquireError::OperationCanceled)]
    #[case::interrupted(inquire::InquireError::OperationInterrupted)]
    fn preserves_prompt_source_and_message(#[case] source: inquire::InquireError) {
        let message = source.to_string();
        let error = InitError::prompt(source);
        assert_eq!(error.to_string(), message);
        assert!(error.source().unwrap().is::<inquire::InquireError>());
    }
}

impl InitError {
    pub fn unexpected_version(version: &str) -> Self {
        Self::UnexpectedVersion(version.to_owned())
    }

    fn unexpected_version_message(version: &str) -> String {
        format!(include_str!("unexpected_version.txt"), version = version)
            .trim_end()
            .to_owned()
    }
}

#[cfg(test)]
mod test_unexpected_version {
    use super::InitError;
    use std::error::Error as StdError;

    #[rstest::rstest]
    #[case::unsupported("19", "unexpected version 19")]
    #[case::invalid("invalid", "unexpected version invalid")]
    fn reports_the_supplied_version(#[case] version: &str, #[case] message: &str) {
        let error = InitError::unexpected_version(version);
        assert_eq!(error.to_string(), message);
        assert!(error.source().is_none());
    }
}
