//! Errors that can encountered when loading the environment variables set by `cargo px`.

#[derive(Debug)]
#[non_exhaustive]
/// An error that can occur when retrieving the value of an env variable set by `cargo px`.
pub enum VarError {
    /// The variable is not set.
    Missing(MissingVarError),
    /// The variable contains invalid Unicode data.
    InvalidUnicode(InvalidUnicodeError),
}

#[derive(Debug)]
/// One of the env variables that should be set by `cargo px` is not set.
pub struct MissingVarError {
    pub(crate) name: &'static str,
    pub(crate) source: std::env::VarError,
}

#[derive(Debug)]
/// One of the env variables that should be set by `cargo px` contains invalid Unicode data.
pub struct InvalidUnicodeError {
    pub(crate) name: &'static str,
    pub(crate) source: std::env::VarError,
}

impl std::fmt::Display for VarError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VarError::Missing(e) => std::fmt::Display::fmt(e, f),
            VarError::InvalidUnicode(e) => std::fmt::Display::fmt(e, f),
        }
    }
}

impl std::error::Error for VarError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            VarError::Missing(e) => Some(e),
            VarError::InvalidUnicode(e) => Some(e),
        }
    }
}

impl std::fmt::Display for MissingVarError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "The environment variable `{}` is missing. Are you running the command through `cargo px`?", self.name)
    }
}

impl std::fmt::Display for InvalidUnicodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "The environment variable `{}` contains invalid Unicode data.",
            self.name
        )
    }
}

impl std::error::Error for MissingVarError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

impl std::error::Error for InvalidUnicodeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}
