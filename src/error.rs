/// The lsmith result type
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Errors that can occur while using lsmith
#[derive(Debug)]
pub enum Error {
    /// An underlying filesystem or I/O operation failed.
    Io(std::io::Error),

    /// The database contains invalid or inconsistent data.
    Corruption(String),

    /// A checksum verification failed.
    ChecksumMismatch,

    /// Data could not be decoded because it has an invalid format.
    InvalidFormat(String),

    /// The supplied key is invalid.
    InvalidKey,

    /// The supplied value is invalid.
    InvalidValue,

    /// The database already exists when creation was requested.
    DatabaseExists,

    /// The database does not exist when opening was requested.
    DatabaseNotFound,

    /// An operation was attempted on a closed database.
    DatabaseClosed,

    /// The database configuration is invalid.
    InvalidConfiguration(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => {
                write!(f, "I/O error: {error}")
            }

            Self::Corruption(message) => {
                write!(f, "database corruption: {message}")
            }

            Self::ChecksumMismatch => {
                write!(f, "checksum mismatch")
            }

            Self::InvalidFormat(message) => {
                write!(f, "invalid format: {message}")
            }

            Self::InvalidKey => {
                write!(f, "invalid key")
            }

            Self::InvalidValue => {
                write!(f, "invalid value")
            }

            Self::DatabaseExists => {
                write!(f, "database already exists")
            }

            Self::DatabaseNotFound => {
                write!(f, "database not found")
            }

            Self::DatabaseClosed => {
                write!(f, "database is closed")
            }

            Self::InvalidConfiguration(message) => {
                write!(f, "invalid database configuration: {message}")
            }
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}