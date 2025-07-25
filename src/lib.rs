use std::path::Path;
use std::time::Instant;

use anyhow::Context;
use codegen_unit::CodegenUnit;
use guppy::graph::{PackageGraph, PackageMetadata};
use targets::determine_targets;

use crate::codegen_unit::{extract_codegen_units, BinaryInvocation};

mod codegen_plan;
mod codegen_unit;
mod config;
mod shell;
mod targets;

pub use shell::{Shell, Verbosity};

/// Find all codegen units in the current workspace and perform code generation for each of them,
/// in an order that takes into account their respective dependency relationships.
#[tracing::instrument(level = tracing::Level::DEBUG, name = "Generate crates", skip(cargo_path))]
pub fn codegen(
    cargo_path: &str,
    working_directory: &Path,
    args: &[String],
    shell: &mut Shell,
) -> Result<(), Vec<anyhow::Error>> {
    let package_graph = package_graph(cargo_path, shell).map_err(|e| vec![e])?;
    let codegen_plan = compute_filtered_codegen_plan(working_directory, args, &package_graph)?;

    let workspace_dir = package_graph
        .workspace()
        .root()
        .canonicalize()
        .context("Failed to get the canonical path to the root directory of this workspace")
        .map_err(|e| vec![e])?;
    for unit in codegen_plan {
        generate_crate(&unit, cargo_path, &workspace_dir, shell).map_err(|e| vec![e])?;
    }

    Ok(())
}

/// Find all codegen units in the current workspace and verify that the associated projects
/// are fresh—i.e. they don't need to be regenerated.
#[tracing::instrument(level = tracing::Level::DEBUG, name = "Verify freshness", skip(cargo_path))]
pub fn verify(
    cargo_path: &str,
    working_directory: &Path,
    args: &[String],
    shell: &mut Shell,
) -> Result<(), Vec<anyhow::Error>> {
    let package_graph = package_graph(cargo_path, shell).map_err(|e| vec![e])?;
    let codegen_plan = compute_filtered_codegen_plan(working_directory, args, &package_graph)?;

    let workspace_dir = package_graph
        .workspace()
        .root()
        .canonicalize()
        .context("Failed to get the canonical path to the root directory of this workspace")
        .map_err(|e| vec![e])?;
    for unit in codegen_plan {
        let Some(verifier) = &unit.verifier else {
            return Err(vec![anyhow::anyhow!(
                "`{}` doesn't define a verifier, therefore we can't verify if it's fresh",
                unit.package_metadata.name()
            )]);
        };
        verify_crate(
            verifier,
            &unit.package_metadata,
            cargo_path,
            &workspace_dir,
            shell,
        )
        .map_err(|e| vec![e])?;
    }

    Ok(())
}

fn compute_filtered_codegen_plan<'a>(
    working_directory: &Path,
    args: &[String],
    package_graph: &'a PackageGraph,
) -> Result<Vec<CodegenUnit<'a>>, Vec<anyhow::Error>> {
    let mut codegen_units = extract_codegen_units(package_graph)?;

    if tracing::event_enabled!(tracing::Level::DEBUG) {
        let codegen_unit_names: Vec<_> = codegen_units
            .iter()
            .map(|unit| unit.package_metadata.name().to_string())
            .collect();
        tracing::debug!(
            ?codegen_unit_names,
            "Determined the list of codegen units in the current workspace"
        );
    }

    let targets = determine_targets(args, working_directory, package_graph);

    if tracing::event_enabled!(tracing::Level::DEBUG) {
        let target_names: Vec<_> = targets
            .iter()
            .map(|id| {
                package_graph
                    .metadata(id)
                    .expect("Unknown package id")
                    .name()
                    .to_owned()
            })
            .collect();
        tracing::debug!(
            ?target_names,
            "Determined the list of target packages for this invocation"
        );
    }

    // Keep only the codegen units that appear in the dependency graph of the targets we've chosen
    if !targets.is_empty() {
        let mut depends_cache = package_graph.new_depends_cache();
        codegen_units.retain(|unit| {
            targets.iter().any(|target_id| {
                unit.package_metadata.id() == target_id
                    || depends_cache
                        .depends_on(target_id, unit.package_metadata.id())
                        .unwrap_or(false)
            })
        });
    }

    if tracing::event_enabled!(tracing::Level::DEBUG) {
        let codegen_unit_names: Vec<_> = codegen_units
            .iter()
            .map(|unit| unit.package_metadata.name().to_string())
            .collect();
        tracing::debug!(
            ?codegen_unit_names,
            "Retaining only the following codegen units for this invocation, based on the target packages"
        );
    }

    codegen_plan::codegen_plan(codegen_units, package_graph)
}

#[tracing::instrument(name = "Verify crate", skip_all, fields(crate_name = %package_metadata.name()))]
fn verify_crate(
    verifier: &BinaryInvocation,
    package_metadata: &PackageMetadata,
    cargo_path: &str,
    workspace_path: &Path,
    shell: &mut Shell,
) -> Result<(), anyhow::Error> {
    let be_quiet = shell.verbosity() == Verbosity::Quiet;

    // Compile verifier
    {
        let timer = Instant::now();
        let _ = shell.status(
            "Compiling",
            format!(
                "`{}`, the verifier for `{}`",
                verifier.binary.name,
                package_metadata.name()
            ),
        );
        let mut cmd = verifier.build_command(cargo_path, be_quiet);
        cmd.env("CARGO_PX_WORKSPACE_ROOT_DIR", workspace_path)
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit());

        let err_msg = || {
            format!(
                "Failed to compile `{}`, the verifier for `{}`",
                verifier.binary.name,
                package_metadata.name()
            )
        };

        let status = cmd.status().with_context(err_msg)?;
        if !status.success() {
            anyhow::bail!(err_msg());
        }
        let _ = shell.status(
            "Compiled",
            format!(
                "`{}`, the verifier for `{}`, in {:.3}s",
                verifier.binary.name,
                package_metadata.name(),
                timer.elapsed().as_secs_f32()
            ),
        );
    }

    // Invoke verifier
    {
        let timer = Instant::now();
        let _ = shell.status("Verifying", format!("`{}`", package_metadata.name()));
        let mut cmd = verifier.run_command(cargo_path, be_quiet);

        cmd.env(
            "CARGO_PX_GENERATED_PKG_MANIFEST_PATH",
            package_metadata.manifest_path(),
        )
        .env("CARGO_PX_WORKSPACE_ROOT_DIR", workspace_path)
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit());

        let err_msg = || {
            format!(
                "Failed to run `{}`, the verifier for `{}`",
                verifier.binary.name,
                package_metadata.name()
            )
        };

        let status = cmd.status().with_context(err_msg)?;
        if !status.success() {
            anyhow::bail!(err_msg());
        }
        let _ = shell.status(
            "Verified",
            format!(
                "`{}` in {:.3}s",
                package_metadata.name(),
                timer.elapsed().as_secs_f32()
            ),
        );
    }
    Ok(())
}

#[tracing::instrument(name = "Generate crate", skip_all, fields(crate_name = %unit.package_metadata.name()))]
fn generate_crate(
    unit: &codegen_unit::CodegenUnit,
    cargo_path: &str,
    workspace_path: &Path,
    shell: &mut Shell,
) -> Result<(), anyhow::Error> {
    let be_quiet = shell.verbosity() == Verbosity::Quiet;

    // Compile generator
    {
        let timer = Instant::now();
        let _ = shell.status(
            "Compiling",
            format!(
                "`{}`, the code generator for `{}`",
                unit.generator.binary.name,
                unit.package_metadata.name()
            ),
        );
        let mut cmd = unit.generator.build_command(cargo_path, be_quiet);
        cmd.env("CARGO_PX_WORKSPACE_ROOT_DIR", workspace_path)
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit());

        let err_msg = || {
            format!(
                "Failed to compile `{}`, the code generator for `{}`",
                unit.generator.binary.name,
                unit.package_metadata.name()
            )
        };

        let status = cmd.status().with_context(err_msg)?;
        if !status.success() {
            anyhow::bail!(err_msg());
        }
        let _ = shell.status(
            "Compiled",
            format!(
                "`{}`, the code generator for `{}`, in {:.3}s",
                unit.generator.binary.name,
                unit.package_metadata.name(),
                timer.elapsed().as_secs_f32()
            ),
        );
    }

    // Invoke generator
    {
        let timer = Instant::now();
        let _ = shell.status("Generating", format!("`{}`", unit.package_metadata.name()));
        let mut cmd = unit.generator.run_command(cargo_path, be_quiet);

        cmd.env(
            "CARGO_PX_GENERATED_PKG_MANIFEST_PATH",
            unit.package_metadata.manifest_path(),
        )
        .env("CARGO_PX_WORKSPACE_ROOT_DIR", workspace_path)
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit());

        let err_msg = || {
            format!(
                "Failed to run `{}`, the code generator for package `{}`",
                unit.generator.binary.name,
                unit.package_metadata.name()
            )
        };

        let status = cmd.status().with_context(err_msg)?;
        if !status.success() {
            anyhow::bail!(err_msg());
        }
        let _ = shell.status(
            "Generated",
            format!(
                "`{}` in {:.3}s",
                unit.package_metadata.name(),
                timer.elapsed().as_secs_f32()
            ),
        );
    }
    Ok(())
}

/// Build the package graph for the current workspace.
#[tracing::instrument(name = "Compute package graph", skip_all)]
fn package_graph(cargo_path: &str, shell: &mut Shell) -> Result<PackageGraph, anyhow::Error> {
    let timer = Instant::now();
    let _ = shell.status("Computing", "package graph");
    let mut metadata_cmd = guppy::MetadataCommand::new();
    metadata_cmd.cargo_path(cargo_path);
    let package_graph = metadata_cmd
        .exec()
        .context("Failed to execute `cargo metadata`")?
        .build_graph()
        .context("Failed to build a package graph starting from the output of `cargo metadata`");
    let _ = shell.status(
        "Computed",
        format!("package graph in {:.3}s", timer.elapsed().as_secs_f32()),
    );
    package_graph
}
