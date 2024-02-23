//! Logic to retrieve and validate codegen units defined in the current workspace.

use crate::config::{GenerateConfig, ManifestMetadata, PxConfig, VerifyConfig};
use anyhow::Context;
use guppy::{
    graph::{BuildTargetKind, PackageGraph, PackageMetadata},
    PackageId,
};

/// A package that relies on `cargo px` for code generation.
#[derive(Debug, Clone)]
pub(crate) struct CodegenUnit<'graph> {
    /// The metadata of the package that requires code generation.
    pub(crate) package_metadata: PackageMetadata<'graph>,
    pub(crate) generator: BinaryInvocation<'graph>,
    pub(crate) verifier: Option<BinaryInvocation<'graph>>,
}

#[derive(Debug, Clone)]
pub(crate) struct BinaryInvocation<'graph> {
    /// The binary to be invoked.
    /// It must be a binary defined within the same workspace.
    pub(crate) binary: WorkspaceBinary<'graph>,
    /// The arguments to be passed to the binary when invoked.
    pub(crate) args: Vec<String>,
}

impl<'graph> BinaryInvocation<'graph> {
    /// Build a `std::process::Command` that invokes the binary.
    pub fn run_command(&self, cargo_path: &str, be_quiet: bool) -> std::process::Command {
        let mut cmd = self.binary.run_command(cargo_path, be_quiet);
        if !self.args.is_empty() {
            cmd.arg("--").args(&self.args);
        }
        cmd
    }

    /// Build a `std::process::Command` that builds the code generator for this
    /// codegen unit.
    pub fn build_command(&self, cargo_path: &str, be_quiet: bool) -> std::process::Command {
        self.binary.build_command(cargo_path, be_quiet)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct WorkspaceBinary<'graph> {
    /// The name of a binary defined within the current workspace.
    pub(crate) name: String,
    /// The package ID of the local package that defines the binary.
    pub(crate) package_id: &'graph PackageId,
    /// The metadata of the local package that defines the binary.
    pub(crate) package_metadata: PackageMetadata<'graph>,
}

impl<'graph> WorkspaceBinary<'graph> {
    /// Build a `std::process::Command` that invokes the binary.
    pub fn run_command(&self, cargo_path: &str, be_quiet: bool) -> std::process::Command {
        let mut cmd = std::process::Command::new(cargo_path);
        cmd.arg("run")
            .arg("--package")
            .arg(self.package_metadata.name())
            .arg("--bin")
            .arg(&self.name)
            .env(
                "CARGO_PX_GENERATED_PKG_MANIFEST_PATH",
                self.package_metadata.manifest_path(),
            );
        if be_quiet {
            cmd.arg("--quiet");
        }
        cmd
    }

    /// Build a `std::process::Command` that builds the binary.
    pub fn build_command(&self, cargo_path: &str, be_quiet: bool) -> std::process::Command {
        let mut cmd = std::process::Command::new(cargo_path);
        cmd.arg("build")
            .arg("--package")
            .arg(self.package_metadata.name())
            .arg("--bin")
            .arg(&self.name);
        if be_quiet {
            cmd.arg("--quiet");
        }
        cmd
    }
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
        let GenerateConfig::CargoWorkspaceBinary(gen_config) = px_config.generate;

        let mut generator_package_id = None;
        for workspace_member in pkg_graph.workspace().iter() {
            if workspace_member.id() == pkg_metadata.id() {
                continue;
            }

            for target in workspace_member.build_targets() {
                if target.kind() == BuildTargetKind::Binary
                    && target.name() == gen_config.generator_name
                {
                    generator_package_id = Some(workspace_member.id());
                    break;
                }
            }
        }

        let Some(generator_package_id) = generator_package_id else {
            anyhow::bail!(
                "There is no binary named `{}` in the workspace, but it's listed as the generator name for package `{}`",
                gen_config.generator_name,
                pkg_metadata.name(),
            );
        };
        let generator_package_metadata =
            pkg_graph.metadata(generator_package_id).with_context(|| {
                format!(
                    "Failed to retrieve the metadata of the package that defines `{}`, \
                            the code generator binary",
                    gen_config.generator_name
                )
            })?;
        let generator = BinaryInvocation {
            binary: WorkspaceBinary {
                name: gen_config.generator_name,
                package_id: generator_package_id,
                package_metadata: generator_package_metadata,
            },
            args: gen_config.generator_args,
        };

        let mut verifier = None;
        if let Some(VerifyConfig::CargoWorkspaceBinary(verify_config)) = px_config.verify {
            let mut verifier_package_id = None;
            for workspace_member in pkg_graph.workspace().iter() {
                if workspace_member.id() == pkg_metadata.id() {
                    continue;
                }

                for target in workspace_member.build_targets() {
                    if target.kind() == BuildTargetKind::Binary
                        && target.name() == verify_config.verifier_name
                    {
                        verifier_package_id = Some(workspace_member.id());
                        break;
                    }
                }
            }

            let Some(verifier_package_id) = verifier_package_id else {
                anyhow::bail!(
                    "There is no binary named `{}` in the workspace, but it's listed as the verifier name for package `{}`",
                    verify_config.verifier_name,
                    pkg_metadata.name(),
                );
            };
            let verifier_package_metadata =
                pkg_graph.metadata(verifier_package_id).with_context(|| {
                    format!(
                        "Failed to retrieve the metadata of the package that defines `{}`, \
                        the verifier binary",
                        verify_config.verifier_name
                    )
                })?;
            verifier = Some(BinaryInvocation {
                binary: WorkspaceBinary {
                    name: verify_config.verifier_name,
                    package_id: verifier_package_id,
                    package_metadata: verifier_package_metadata,
                },
                args: verify_config.verifier_args,
            });
        }

        Ok(CodegenUnit {
            package_metadata: pkg_metadata,
            generator,
            verifier,
        })
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
                    "Failed to deserialize `cargo px`'s codegen configuration from the manifest of `{}`",
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
