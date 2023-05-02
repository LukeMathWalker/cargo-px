//! The configuration that `px` expects to find in the `Cargo.toml` manifests of 
//! the packages that require code generation.

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct ManifestMetadata {
    #[serde(default)]
    pub(crate) px: Option<PxConfig>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct PxConfig {
    pub(crate) generate: GenerateConfig,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
#[serde(tag = "generator_type", rename_all = "snake_case")]
pub(crate) enum GenerateConfig {
    /// The code generation step is performed by invoking a binary defined within the same workspace.
    CargoWorkspaceBinary(CargoBinaryGeneratorConfig),
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct CargoBinaryGeneratorConfig {
    /// The name of the binary to be invoked in order to perform code generation.
    ///
    /// It must be a binary defined within the same workspace.
    pub(crate) generator_name: String,
    #[serde(default)]
    /// The arguments to be passed to the generator binary.
    pub(crate) generator_args: Vec<String>,
}
