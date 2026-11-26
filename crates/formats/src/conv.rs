//! Turning each library's errors into ArchiveCraft's one error type.

use archivecraft_model::Error;

/// `.ac()` converts a library result into ours, like `?` would with a `From` impl.
pub(crate) trait Ac<T> {
    fn ac(self) -> Result<T, Error>;
}

impl<T> Ac<T> for Result<T, zip::result::ZipError> {
    fn ac(self) -> Result<T, Error> {
        self.map_err(zip_err)
    }
}

impl<T> Ac<T> for Result<T, sevenz_rust2::Error> {
    fn ac(self) -> Result<T, Error> {
        self.map_err(sevenz_err)
    }
}

#[cfg(feature = "rar")]
impl<T> Ac<T> for Result<T, rars::Error> {
    fn ac(self) -> Result<T, Error> {
        self.map_err(rar_err)
    }
}

pub(crate) fn zip_err(e: zip::result::ZipError) -> Error {
    {
        use zip::result::ZipError;
        match e {
            ZipError::Io(io) => Error::from(io),
            ZipError::InvalidPassword => Error::WrongPassword,
            other => {
                let s = other.to_string();
                let l = s.to_lowercase();
                if l.contains("password") {
                    Error::PasswordRequired
                } else if l.contains("not supported") || l.contains("unsupported") {
                    Error::Unsupported(s)
                } else {
                    Error::Corrupt(s)
                }
            }
        }
    }
}

pub(crate) fn sevenz_err(e: sevenz_rust2::Error) -> Error {
    {
        use sevenz_rust2::Error as E;
        match e {
            E::PasswordRequired => Error::PasswordRequired,
            E::MaybeBadPassword(_) => Error::WrongPassword,
            E::Io(io, _) => Error::from(io),
            E::FileOpen(io, _) => Error::Io(io),
            E::ChecksumVerificationFailed | E::NextHeaderCrcMismatch => {
                Error::Corrupt("checksum does not match".into())
            }
            E::UnsupportedCompressionMethod(m) => Error::Unsupported(format!("7z method {m}")),
            other => {
                let s = other.to_string();
                if s.to_lowercase().contains("password") {
                    Error::WrongPassword
                } else {
                    Error::Corrupt(s)
                }
            }
        }
    }
}

#[cfg(feature = "rar")]
pub(crate) fn rar_err(e: rars::Error) -> Error {
    {
        use rars::ErrorKind as K;
        match e.kind() {
            K::PasswordRequired => Error::PasswordRequired,
            K::BadPassword => Error::WrongPassword,
            K::Cancelled => Error::Cancelled,
            K::UnsupportedFormat | K::UnsupportedFeature => Error::Unsupported(e.to_string()),
            _ => Error::Corrupt(e.to_string()),
        }
    }
}
