#![doc = include_str!("../README.md")]
use std::path::PathBuf;

use crate::error::{InvalidUnicodeError, MissingVarError, VarError};

pub mod error;

/// The name of the environment variable that contains the path to the root directory
/// of the current workspace.
pub const WORKSPACE_ROOT_DIR_ENV: &str = "CARGO_PX_WORKSPACE_ROOT_DIR";
/// The name of the environment variable that contains the path to the manifest
/// of the crate that must be generated.
pub const GENERATED_PKG_MANIFEST_PATH_ENV: &str = "CARGO_PX_GENERATED_PKG_MANIFEST_PATH";

/// Retrieve the path to the workspace root directory.
///
/// It returns an error if the variable is not set or if it contains invalid Unicode data.
pub fn workspace_root_dir() -> Result<PathBuf, VarError> {
    px_env_var(WORKSPACE_ROOT_DIR_ENV).map(PathBuf::from)
}

/// Retrieve the path to the manifest of the crate that must be generated.
///
/// It returns an error if the variable is not set or if it contains invalid Unicode data.
pub fn generated_pkg_manifest_path() -> Result<PathBuf, VarError> {
    px_env_var(GENERATED_PKG_MANIFEST_PATH_ENV).map(PathBuf::from)
}

/// Retrieve the value of an env variable set by `cargo px`.
///
/// It returns an error if the variable is not set or if it contains invalid Unicode data.
fn px_env_var(name: &'static str) -> Result<String, VarError> {
    use std::env::{var, VarError};

    var(name).map_err(|e| match e {
        VarError::NotPresent => {
            crate::error::VarError::Missing(MissingVarError { name, source: e })
        }
        VarError::NotUnicode(_) => {
            crate::error::VarError::InvalidUnicode(InvalidUnicodeError { name, source: e })
        }
    })
}
