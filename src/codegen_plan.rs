use std::fmt::Write;

use ahash::{HashMap, HashMapExt, HashSet, HashSetExt};
use guppy::{
    graph::{DependencyDirection, PackageGraph},
    PackageId,
};
use petgraph::{
    stable_graph::{IndexType, NodeIndex, StableDiGraph},
    visit::DfsPostOrder,
    Direction::Incoming,
};

use crate::codegen_unit::CodegenUnit;

/// Return a codegen plan: a vector of codegen units in an order that takes into account
/// their respective dependency relationships—i.e. you can safely invoke them in order
/// and each codegen unit will be generated after all the codegen units it depends on.
pub(crate) fn codegen_plan<'graph>(
    codegen_units: Vec<CodegenUnit<'graph>>,
    package_graph: &'graph PackageGraph,
) -> Result<Vec<CodegenUnit<'graph>>, Vec<anyhow::Error>> {
    Ok(AugmentedPackageGraph::new(codegen_units, package_graph)?.codegen_plan())
}

#[derive(Debug)]
/// A dependency graph augmented with information about the code generation units.
/// In particular, an `A -> B` edge means that `A` depends on `B` via a non-dev dependency.
///
/// The graph includes all packages defined in the workspace and packages that depend on
/// a workspace crate.  
/// It is therefore likely to be much smaller than the [`PackageGraph`] it was built from.
struct AugmentedPackageGraph<'graph> {
    /// The dependency graph.
    dep_graph: StableDiGraph<PackageId, EdgeMetadata<'graph>>,
}

#[derive(Debug)]
enum EdgeMetadata<'graph> {
    DependsOn,
    IsGeneratedBy(CodegenUnit<'graph>),
}

impl<'graph> AugmentedPackageGraph<'graph> {
    fn new(
        codegen_units: Vec<CodegenUnit<'graph>>,
        package_graph: &'graph PackageGraph,
    ) -> Result<Self, Vec<anyhow::Error>> {
        // A map from package ID to node ID in the dependency graph.
        let mut pkg_id2node_id = HashMap::new();
        let mut processed_pkg_ids = HashSet::new();
        let mut dep_graph =
            petgraph::stable_graph::StableDiGraph::<PackageId, EdgeMetadata<'graph>>::new();
        let mut to_be_visited = package_graph.workspace().member_ids().collect::<Vec<_>>();
        while let Some(pkg_id) = to_be_visited.pop() {
            if processed_pkg_ids.contains(&pkg_id) {
                continue;
            }

            let node_id = if pkg_id2node_id.contains_key(pkg_id) {
                pkg_id2node_id[pkg_id]
            } else {
                let node_id = dep_graph.add_node(pkg_id.clone());
                pkg_id2node_id.insert(pkg_id.clone(), node_id);
                node_id
            };

            let pkg_metadata = package_graph.metadata(pkg_id).unwrap();

            // We only care about the portion of the bigger package graph that includes the local
            // workspace crates.
            // Therefore we look for _reverse_ dependencies here—i.e. we avoid adding any package
            // to the graph that does not depend on a workspace crate.
            let pkg_deps = pkg_metadata.direct_links_directed(DependencyDirection::Reverse);
            for dep in pkg_deps {
                if dep.dev_only() {
                    continue;
                }
                let dep_pkg_id = dep.from().id();

                let dep_node_id = if pkg_id2node_id.contains_key(dep_pkg_id) {
                    pkg_id2node_id[dep_pkg_id]
                } else {
                    let node_id = dep_graph.add_node(dep_pkg_id.clone());
                    pkg_id2node_id.insert(dep_pkg_id.clone(), node_id);
                    node_id
                };

                dep_graph.update_edge(dep_node_id, node_id, EdgeMetadata::DependsOn);

                if !processed_pkg_ids.contains(&dep_pkg_id) {
                    to_be_visited.push(dep_pkg_id);
                }
            }

            processed_pkg_ids.insert(pkg_id);
        }

        // Add edges from the generator package to the respective codegen units.
        for codegen_unit in codegen_units {
            let target_node_id = pkg_id2node_id[codegen_unit.generator_package_id];
            let codegen_node_id = pkg_id2node_id[codegen_unit.package_metadata.id()];
            dep_graph.update_edge(
                codegen_node_id,
                target_node_id,
                EdgeMetadata::IsGeneratedBy(codegen_unit),
            );
        }

        // Cyclic dependencies are not allowed.
        let cycles = find_cycles(&dep_graph);
        if !cycles.is_empty() {
            return Err(cycles
                .into_iter()
                .map(|cycle| cyclic_dependency_error(&cycle, &dep_graph))
                .collect());
        }

        Ok(Self { dep_graph })
    }

    /// Returns the set of binary invocations that need to be executed in order to build the
    /// codegen units.
    ///
    /// The returned set is ordered such that the codegen units can be built in an order that
    /// takes into account their dependency relationships.
    pub fn codegen_plan(&self) -> Vec<CodegenUnit<'graph>> {
        let mut codegen_plan = Vec::new();
        let mut sources = self.dep_graph.externals(Incoming).collect::<Vec<_>>();
        // Always true since the graph is acyclic.
        assert!(!sources.is_empty());
        let source_seed = sources.pop().unwrap();
        let mut dfs = DfsPostOrder::new(&self.dep_graph, source_seed);
        loop {
            while let Some(node_index) = dfs.next(&self.dep_graph) {
                let dependent_edges = self.dep_graph.edges_directed(node_index, Incoming);
                for dependent_edge in dependent_edges {
                    if let EdgeMetadata::IsGeneratedBy(codegen_unit) = dependent_edge.weight() {
                        codegen_plan.push(codegen_unit.to_owned());
                    }
                }
            }

            if let Some(next_source_seed) = sources.pop() {
                dfs.move_to(next_source_seed);
            } else {
                break;
            }
        }

        codegen_plan
    }
}

fn cyclic_dependency_error(
    cycle: &[NodeIndex],
    graph: &StableDiGraph<PackageId, EdgeMetadata>,
) -> anyhow::Error {
    let mut error_msg = "There is a cyclic dependency in your workspace: this is not allowed!\n\
        The cycle looks like this:".to_string();
    for (i, node_id) in cycle.iter().enumerate() {
        writeln!(&mut error_msg).unwrap();
        let dependent_id = if i == 0 {
            *cycle.last().unwrap()
        } else {
            cycle[i - 1]
        };
        let dependent = graph[dependent_id].repr();
        let edge_id = graph.find_edge(dependent_id, *node_id).unwrap();
        let relationship = graph.edge_weight(edge_id).unwrap();
        let relationship = match relationship {
            EdgeMetadata::DependsOn => "depends on",
            EdgeMetadata::IsGeneratedBy(_) => "is generated by",
        };
        let dependency = graph[*node_id].repr();
        write!(
            &mut error_msg,
            "- `{dependent}` {relationship} `{dependency}`",
        )
        .unwrap();
    }
    anyhow::anyhow!(error_msg) 
}

/// Return all the cycles in the graph.
///
/// It's an empty vector if the graph is acyclic.
fn find_cycles<N, E, Ix>(graph: &StableDiGraph<N, E, Ix>) -> Vec<Vec<NodeIndex<Ix>>>
where
    Ix: IndexType,
{
    fn dfs<N, E, Ix>(
        node_index: NodeIndex<Ix>,
        graph: &StableDiGraph<N, E, Ix>,
        visited: &mut HashSet<NodeIndex<Ix>>,
        stack: &mut Vec<NodeIndex<Ix>>,
        cycles: &mut Vec<Vec<NodeIndex<Ix>>>,
    ) where
        Ix: IndexType,
    {
        visited.insert(node_index);
        stack.push(node_index);

        for neighbour_index in graph.neighbors_directed(node_index, petgraph::Direction::Outgoing) {
            if !visited.contains(&neighbour_index) {
                dfs(neighbour_index, graph, visited, stack, cycles);
            } else if let Some(cycle_start) = stack.iter().position(|&x| x == neighbour_index) {
                let cycle = stack[cycle_start..].to_vec();
                cycles.push(cycle);
            }
        }

        stack.pop();
    }

    let mut visited = HashSet::new();
    let mut stack = Vec::new();
    let mut cycles = Vec::new();

    for node_index in graph.node_indices() {
        if !visited.contains(&node_index) {
            dfs(node_index, graph, &mut visited, &mut stack, &mut cycles);
        }
    }

    cycles
}
