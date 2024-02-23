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
    pub(crate) verify: Option<VerifyConfig>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
#[serde(tag = "generator_type", rename_all = "snake_case")]
pub(crate) enum GenerateConfig {
    /// The code generation step is performed by invoking a binary defined within the same workspace.
    CargoWorkspaceBinary(CargoBinaryGeneratorConfig),
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
#[serde(tag = "verifier_type", rename_all = "snake_case")]
pub(crate) enum VerifyConfig {
    /// The verification step is performed by invoking a binary defined within the same workspace.
    CargoWorkspaceBinary(CargoBinaryVerifierConfig),
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct CargoBinaryGeneratorConfig {
    /// The name of the binary to be invoked to perform code generation.
    ///
    /// It must be a binary defined within the same workspace.
    pub(crate) generator_name: String,
    #[serde(default)]
    /// The arguments to be passed to the generator binary.
    pub(crate) generator_args: Vec<String>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct CargoBinaryVerifierConfig {
    /// The name of the binary to be invoked to verify the freshness of
    /// the generated project.
    ///
    /// It must be a binary defined within the same workspace.
    pub(crate) verifier_name: String,
    #[serde(default)]
    /// The arguments to be passed to the verifier binary.
    pub(crate) verifier_args: Vec<String>,
}
