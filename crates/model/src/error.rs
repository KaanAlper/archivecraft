//! One error type for every archive operation.

use std::fmt;
use std::io;

/// Everything that can go wrong while working with an archive.
#[derive(Debug)]
pub enum Error {
    /// Reading or writing a file failed.
    Io(io::Error),
    /// The archive (or an item in it) is locked and no password was given.
    PasswordRequired,
    /// The password given does not unlock the archive.
    WrongPassword,
    /// The file is of a kind this program does not handle, or the request is not possible for it.
    Unsupported(String),
    /// The archive data is damaged.
    Corrupt(String),
    /// The person stopped the operation.
    Cancelled,
    /// Anything else, with a message.
    Other(String),
}

pub type Result<T> = std::result::Result<T, Error>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "file system problem: {e}"),
            Error::PasswordRequired => write!(f, "this archive is locked; a password is needed"),
            Error::WrongPassword => write!(f, "the password does not unlock this archive"),
            Error::Unsupported(s) => write!(f, "not supported: {s}"),
            Error::Corrupt(s) => write!(f, "damaged data: {s}"),
            Error::Cancelled => write!(f, "stopped before it finished"),
            Error::Other(s) => write!(f, "{s}"),
        }
    }
}

impl std::error::Error for Error {}

impl Error {
    pub fn is_password(&self) -> bool {
        matches!(self, Error::PasswordRequired | Error::WrongPassword)
    }
    pub fn other(s: impl Into<String>) -> Self {
        Error::Other(s.into())
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        // Errors coming back through `Read`/`Write` adapters may wrap one of ours.
        if e.get_ref().is_some_and(|inner| inner.is::<Error>()) {
            let inner = e.into_inner().unwrap();
            return *inner.downcast::<Error>().unwrap();
        }
        let msg = e.to_string().to_lowercase();
        if msg.contains("password") {
            return Error::WrongPassword;
        }
        if msg.contains("checksum") || msg.contains("crc") {
            return Error::Corrupt(e.to_string());
        }
        Error::Io(e)
    }
}

impl From<Error> for io::Error {
    fn from(e: Error) -> Self {
        match e {
            Error::Io(e) => e,
            other => io::Error::other(other),
        }
    }
}
