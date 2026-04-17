//! Error types for genpdfi.

use std::error;
use std::fmt;
use std::io;

/// Helper trait for creating [`Error`][] instances.
pub trait Context<T> {
    /// Adds a context message to the error.
    fn context(self, msg: impl Into<String>) -> Result<T, Error>;
    /// Adds a context message to the error using a callback.
    fn with_context<F, S>(self, cb: F) -> Result<T, Error>
    where
        F: Fn() -> S,
        S: Into<String>;
}

impl<T, E: Into<ErrorKind>> Context<T> for Result<T, E> {
    fn context(self, msg: impl Into<String>) -> Result<T, Error> {
        self.map_err(|err| Error::new(msg, err))
    }
    fn with_context<F, S>(self, cb: F) -> Result<T, Error>
    where
        F: Fn() -> S,
        S: Into<String>,
    {
        self.map_err(move |err| Error::new(cb(), err))
    }
}

/// An error that occured in a genpdfi function.
#[derive(Debug)]
pub struct Error {
    msg: String,
    kind: ErrorKind,
}

impl Error {
    /// Creates a new error.
    pub fn new(msg: impl Into<String>, kind: impl Into<ErrorKind>) -> Error {
        Error {
            msg: msg.into(),
            kind: kind.into(),
        }
    }
    /// Returns the error kind.
    pub fn kind(&self) -> &ErrorKind {
        &self.kind
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.msg)
    }
}

impl error::Error for Error {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match &self.kind {
            ErrorKind::IoError(err) => Some(err),
            ErrorKind::RusttypeError(err) => Some(err),
            #[cfg(feature = "images")]
            ErrorKind::ImageError(err) => Some(err),
            _ => None,
        }
    }
}

/// The kind of an error.
#[derive(Debug)]
#[non_exhaustive]
pub enum ErrorKind {
    /// Internal error.
    Internal,
    /// Invalid data.
    InvalidData,
    /// Invalid font.
    InvalidFont,
    /// Page size exceeded.
    PageSizeExceeded,
    /// Unsupported encoding.
    UnsupportedEncoding,
    /// IO error.
    IoError(io::Error),
    /// PDF error.
    PdfError(String),
    /// Rusttype error.
    RusttypeError(rusttype::Error),
    /// Image error.
    #[cfg(feature = "images")]
    ImageError(image::ImageError),
}

impl From<io::Error> for ErrorKind {
    fn from(error: io::Error) -> ErrorKind {
        ErrorKind::IoError(error)
    }
}

impl From<String> for ErrorKind {
    fn from(error: String) -> ErrorKind {
        ErrorKind::PdfError(error)
    }
}

impl From<rusttype::Error> for ErrorKind {
    fn from(error: rusttype::Error) -> ErrorKind {
        ErrorKind::RusttypeError(error)
    }
}

#[cfg(feature = "images")]
impl From<image::ImageError> for ErrorKind {
    fn from(error: image::ImageError) -> ErrorKind {
        ErrorKind::ImageError(error)
    }
}
