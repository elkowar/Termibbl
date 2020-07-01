pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    SendError(String),
    CrosstermError(crossterm::ErrorKind),
    IOError(std::io::Error),
    WebSocketError(tungstenite::error::Error),
}

impl<T> From<tokio::sync::mpsc::error::SendError<T>> for Error {
    fn from(e: tokio::sync::mpsc::error::SendError<T>) -> Self {
        Error::SendError(e.to_string())
    }
}
impl From<crossterm::ErrorKind> for Error {
    fn from(e: crossterm::ErrorKind) -> Self {
        Error::CrosstermError(e)
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::IOError(e)
    }
}

impl From<tungstenite::error::Error> for Error {
    fn from(e: tungstenite::error::Error) -> Self {
        Error::WebSocketError(e)
    }
}
