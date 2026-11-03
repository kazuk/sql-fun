use crate::init::step::StepCompletion;
#[derive(Debug, thiserror::Error)]
pub enum InitError {
    #[error("{0}")]
    Setup(#[from] anyhow::Error),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Prompt(#[from] inquire::InquireError),
}

impl InitError {
    pub(super) const fn navigation(&self) -> Option<StepCompletion> {
        match self {
            Self::Prompt(inquire::InquireError::OperationCanceled) => Some(StepCompletion::back()),
            Self::Prompt(inquire::InquireError::OperationInterrupted) => {
                Some(StepCompletion::cancelled())
            }
            _ => None,
        }
    }
    #[track_caller]
    pub const fn io<T>(error: std::io::Error) -> Result<T, Self> {
        Err(Self::Io(error))
    }
}

#[cfg(test)]
mod test_io {
    use crate::init::error::InitError;
    use std::{error::Error as StdError, io::ErrorKind};

    #[rstest::rstest]
    #[case::permission_denied(ErrorKind::PermissionDenied)]
    #[case::not_a_directory(ErrorKind::NotADirectory)]
    fn preserves_io_source_and_message(#[case] kind: ErrorKind) {
        let error = InitError::io::<()>(std::io::Error::new(kind, "create failed")).unwrap_err();
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
    #[track_caller]
    pub const fn prompt<T>(error: inquire::InquireError) -> Result<T, Self> {
        Err(Self::Prompt(error))
    }
}

#[cfg(test)]
mod test_prompt {
    use crate::init::error::InitError;
    use std::error::Error as StdError;

    #[rstest::rstest]
    #[case::cancelled(inquire::InquireError::OperationCanceled)]
    #[case::interrupted(inquire::InquireError::OperationInterrupted)]
    fn preserves_prompt_source_and_message(#[case] source: inquire::InquireError) {
        let message = source.to_string();
        let error = InitError::prompt::<()>(source).unwrap_err();
        assert_eq!(error.to_string(), message);
        assert!(error.source().unwrap().is::<inquire::InquireError>());
    }
}
