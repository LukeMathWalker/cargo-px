use std::path::Path;
use std::time::Instant;

use anyhow::Context;
use guppy::graph::PackageGraph;

use crate::codegen_unit::extract_codegen_units;
mod codegen_plan;
mod codegen_unit;
mod config;
mod shell;

pub use shell::Shell;

/// Find all codegen units in the current workspace and perform code generation for each of them,
/// in a order that takes into account their respective dependency relationships.
#[tracing::instrument(level = tracing::Level::DEBUG, name = "Generate crates", skip(cargo_path))]
pub fn codegen(cargo_path: &str, shell: &mut Shell) -> Result<(), Vec<anyhow::Error>> {
    let package_graph = package_graph(cargo_path, shell).map_err(|e| vec![e])?;
    let codegen_units = extract_codegen_units(&package_graph)?;
    let codegen_plan = codegen_plan::codegen_plan(codegen_units, &package_graph)?;

    let workspace_dir = package_graph.workspace().root().canonicalize().unwrap();
    for unit in codegen_plan {
        generate_crate(&unit, cargo_path, &workspace_dir).map_err(|e| vec![e])?;
    }

    Ok(())
}

#[tracing::instrument(name = "Generate crate", skip_all, fields(crate_name = %unit.package_metadata.name()))]
fn generate_crate(
    unit: &codegen_unit::CodegenUnit,
    cargo_path: &str,
    workspace_path: &Path,
) -> Result<(), anyhow::Error> {
    let mut cmd = unit.command(cargo_path);
    cmd.env("CARGO_PX_WORKSPACE_ROOT_DIR", workspace_path)
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit());

    let err_msg = || {
        format!(
        "Failed to execute code generator for package `{}`.\nCheck the output above to diagnose what went wrong.",
        unit.package_metadata.name()
    )
    };

    let status = cmd.status().with_context(err_msg)?;
    if !status.success() {
        anyhow::bail!(err_msg());
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
