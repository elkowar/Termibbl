use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("send error: `{0}`")]
    SendError(String),
    #[error("task complete error")]
    TaskError(#[from] tokio::task::JoinError),
    #[error("crossterm error")]
    TermError(#[from] crossterm::ErrorKind),
    #[error("IO error")]
    IOError(#[from] std::io::Error),
}

impl<T> From<tokio::sync::mpsc::error::SendError<T>> for Error {
    fn from(e: tokio::sync::mpsc::error::SendError<T>) -> Self { Error::SendError(e.to_string()) }
}

impl<T> From<tokio::sync::mpsc::error::TrySendError<T>> for Error {
    fn from(e: tokio::sync::mpsc::error::TrySendError<T>) -> Self {
        Error::SendError(e.to_string())
    }
}
