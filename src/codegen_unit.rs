//! Logic to retrieve and validate codegen units defined in the current workspace.

use crate::config::{GenerateConfig, ManifestMetadata, PxConfig};
use guppy::{
    graph::{BuildTargetKind, PackageGraph, PackageMetadata},
    PackageId,
};

/// A package that relies on `cargo px` for code generation.
#[derive(Debug, Clone)]
pub(crate) struct CodegenUnit<'graph> {
    /// The metadata of the package that requires code generation.
    pub(crate) package_metadata: PackageMetadata<'graph>,
    /// The name of the binary to be invoked in order to perform code generation.
    /// It must be a binary defined within the same workspace.
    pub(crate) generator_name: String,
    /// The arguments to be passed to the generator binary.
    pub(crate) generator_args: Vec<String>,
    /// The package ID of the package that defines the binary to be invoked
    /// in order to perform code generation.
    pub(crate) generator_package_id: &'graph PackageId,
}

impl<'graph> CodegenUnit<'graph> {
    /// Build a `CodegenUnit` from the given `px_config` and `pkg_metadata`.
    ///
    /// It returns an error if the `px_config` points to a binary that is not defined
    /// in the same workspace.
    pub(crate) fn new(
        px_config: PxConfig,
        pkg_metadata: PackageMetadata<'graph>,
        pkg_graph: &'graph PackageGraph,
    ) -> Result<CodegenUnit<'graph>, anyhow::Error> {
        let GenerateConfig::CargoWorkspaceBinary(px_config) = px_config.generate;

        let mut generator_package_id = None;
        for workspace_member in pkg_graph.workspace().iter() {
            if workspace_member.id() == pkg_metadata.id() {
                continue;
            }

            for target in workspace_member.build_targets() {
                if target.kind() == BuildTargetKind::Binary
                    && target.name() == px_config.generator_name
                {
                    generator_package_id = Some(workspace_member.id());
                    break;
                }
            }
        }

        match generator_package_id {
            Some(generator_package_id) => Ok(CodegenUnit {
                package_metadata: pkg_metadata,
                generator_name: px_config.generator_name,
                generator_args: px_config.generator_args,
                generator_package_id,
            }),
            None => {
                let error = anyhow::anyhow!(
                    "There is no binary named `{}` in the workspace, but it's listed as the generator name for package `{}`",
                    px_config.generator_name,
                    pkg_metadata.name(),
                );
                Err(error)
            }
        }
    }

    /// Build a `std::process::Command` that invokes the code generator for this
    /// codegen unit.
    pub fn run_command(&self, cargo_path: &str) -> std::process::Command {
        let mut cmd = std::process::Command::new(cargo_path);
        cmd.arg("run")
            .arg("--bin")
            .arg(&self.generator_name)
            .args(&self.generator_args)
            .env(
                "CARGO_PX_GENERATED_PKG_MANIFEST_PATH",
                self.package_metadata.manifest_path(),
            );
        cmd
    }

    /// Build a `std::process::Command` that builds the code generator for this
    /// codegen unit.
    pub fn build_command(&self, cargo_path: &str) -> std::process::Command {
        let mut cmd = std::process::Command::new(cargo_path);
        cmd.arg("build").arg("--bin").arg(&self.generator_name);
        cmd
    }
}

/// Retrieve all packages in the current workspace that require code generation.
pub(crate) fn extract_codegen_units(
    pkg_graph: &PackageGraph,
) -> Result<Vec<CodegenUnit>, Vec<anyhow::Error>> {
    let workspace = pkg_graph.workspace();
    let mut codegen_units = vec![];
    let mut errors = vec![];
    for p_metadata in workspace.iter() {
        let raw_metadata = p_metadata.metadata_table().to_owned();
        match serde_json::from_value::<Option<ManifestMetadata>>(raw_metadata) {
            Ok(metadata) => {
                let Some(metadata) = metadata else {
                    continue;
                };
                let Some(px_config) = metadata.px else {
                    continue;
                };
                match CodegenUnit::new(px_config, p_metadata, pkg_graph) {
                    Ok(codegen_unit) => codegen_units.push(codegen_unit),
                    Err(e) => errors.push(e),
                }
            }
            Err(e) => {
                let e = anyhow::anyhow!(e).context(format!(
                    "Failed to deserialize `cargo px`'s configuration from the manifest of `{}`",
                    p_metadata.name(),
                ));
                errors.push(e)
            }
        };
    }
    if !errors.is_empty() {
        Err(errors)
    } else {
        Ok(codegen_units)
    }
}
