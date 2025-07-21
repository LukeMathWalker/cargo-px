use std::path::Path;

use clap::{Arg, Command};
use guppy::{graph::PackageGraph, PackageId};

/// Determine which sub-units should be built from the package graph.
///
/// We implement a simplified version of the general algorithm in `cargo`. We determine the target packages based on:
///
/// - The `-p`/`--package` flag, which specifies a list of package specs to be considered.
/// - The current working directory, if no package specs are specified.
///
/// But we assume that the specified package specs refer to packages in the workspace. If not, we fall back to performing
/// codegen for everything.
pub(crate) fn determine_targets(
    args: &[String],
    working_directory: &Path,
    package_graph: &PackageGraph,
) -> Vec<PackageId> {
    // TODO: Handle other forms of package selection in `cargo`:
    //   - --workspace / --exclude
    //   - --manifest-path
    //   - Target selection via --bin/--lib/etc.

    let package_specs = extract_package_filters(args);

    if tracing::event_enabled!(tracing::Level::DEBUG) {
        if package_specs.is_empty() {
            tracing::debug!("No package specs provided, determining the target based on the current working directory");
        } else {
            tracing::debug!(
                ?package_specs,
                "Extracted the following package specs for this invocation"
            );
        }
    }

    if package_specs.is_empty() {
        return find_implicit_target(working_directory, package_graph)
            .map(|id| vec![id])
            .unwrap_or_default();
    }

    // Collect the package IDs for the specified package specs.
    let mut package_ids = Vec::new();
    for spec in package_specs {
        if let Ok(package) = package_graph.workspace().member_by_name(&spec) {
            package_ids.push(package.id().clone());
        } else {
            // If any spec does not match a workspace package, fall back to performing codegen for everything.
            return vec![];
        }
    }

    package_ids
}

/// If no package specs have been provided, determine the package based on the working directory.
///
/// We will build the package whose manifest file is closest to the current working directory.
fn find_implicit_target(
    working_directory: &Path,
    package_graph: &PackageGraph,
) -> Option<PackageId> {
    let workspace_root = package_graph.workspace().root();
    // All workspace paths in the graph are relative to the workspace root.
    let working_directory = working_directory
        .strip_prefix(workspace_root)
        .unwrap_or(working_directory);
    package_graph
        .workspace()
        .iter_by_path()
        .filter_map(|(path, package_metadata)| {
            if let Ok(suffix) = working_directory.strip_prefix(path) {
                Some((package_metadata, suffix.components().count()))
            } else {
                None
            }
        })
        .min_by_key(|(_, count)| *count)
        .map(|(package_metadata, _)| package_metadata.id().to_owned())
}

/// Check if the user has specified a list of package specs to be considered.
fn extract_package_filters(args: &[String]) -> Vec<String> {
    let Ok(matches) = Command::new("px")
        .no_binary_name(true)
        .arg(
            Arg::new("package")
                .short('p')
                .long("package")
                .num_args(1)
                .action(clap::ArgAction::Append)
                .help("Package(s) to operate on"),
        )
        .allow_external_subcommands(true)
        .dont_collapse_args_in_usage(true)
        // Skip `px <sub_command>`
        .try_get_matches_from(&args[2..])
    else {
        tracing::debug!("Failed to match `-p`/`--package` arguments");
        return Vec::new();
    };
    matches
        .get_many::<String>("package")
        .map(|vals| vals.cloned().collect())
        .unwrap_or_default()
}
