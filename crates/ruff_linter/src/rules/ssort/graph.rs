use std::collections::VecDeque;
use std::fmt;

use rustc_hash::{FxHashMap, FxHashSet};

use ruff_python_ast::name::Name;
use ruff_python_ast::visitor::{Visitor, walk_expr};
use ruff_python_ast::{Expr, Identifier, Stmt};

/// An error indicating that a cycle was detected in the dependency graph.
#[derive(Debug)]
pub(super) struct CycleError;

/// Allows CycleError to be treated as a standard error.
impl std::error::Error for CycleError {}

/// Allows CycleError to be formatted for display.
impl fmt::Display for CycleError {
    /// Format the cycle error for display.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Cycle detected in dependency graph")
    }
}

/// A function/class definition in the dependency graph.
#[derive(Debug)]
pub(super) struct Node<'a> {
    /// The index of the function/class in the original statement list.
    pub(crate) index: usize,

    /// The name of the function/class.
    pub(crate) name: &'a Identifier,

    /// The statement corresponding to the function/class definition.
    pub(crate) stmt: &'a Stmt,
}

/// A visitor that collects all variable names referenced in expressions.
#[derive(Debug)]
struct DependencyVisitor<'a> {
    /// The set of variable names referenced.
    names: FxHashSet<&'a Name>,
}

/// Allows DependencyVisitor to be used as a visitor for AST nodes.
impl<'a> Visitor<'a> for DependencyVisitor<'a> {
    /// Visit an expression node during AST traversal.
    ///
    /// ## Arguments
    /// * `expr` - The expression node to visit.
    fn visit_expr(&mut self, expr: &'a Expr) {
        // If the expression is a reference, add it to the set
        if let Expr::Name(name) = expr {
            self.names.insert(&name.id);
        }
        walk_expr(self, expr);
    }
}

/// Build a dependency graph for a set of nodes based on references within their bodies.
///
/// ## Arguments
/// * `nodes` - The list of function/class nodes extracted from the module.
///
/// ## Returns
/// A mapping from each node index to the set of node indices it depends on.
pub(super) fn build_dependency_graph(nodes: &[Node]) -> FxHashMap<usize, FxHashSet<usize>> {
    // Cache the function names to their index in the nodes list
    let name_to_index: FxHashMap<&Name, usize> = nodes
        .iter()
        .map(|node| (&node.name.id, node.index))
        .collect();

    // For every node, find its dependencies and build a graph
    let mut graph: FxHashMap<usize, FxHashSet<usize>> = FxHashMap::default();
    for node in nodes {
        // Collect the names referenced in the function body
        let mut visitor = DependencyVisitor {
            names: FxHashSet::default(),
        };
        visitor.visit_stmt(node.stmt);

        // Determine what nodes this function depends on
        let dependencies: FxHashSet<usize> = visitor
            .names
            .iter()
            .filter_map(|&name| name_to_index.get(name).copied())
            .collect();
        graph.insert(node.index, dependencies);
    }
    graph
}

/// Perform a topological sort on the dependency graph.
///
/// This implementation uses Kahn's algorithm to produce an ordering of nodes
/// such that for every directed edge from node A to node B, A comes before B.
/// See <https://en.wikipedia.org/wiki/Topological_sorting> for more details.
///
/// ## Arguments
/// * `nodes` - The list of function/class nodes extracted from the module.
/// * `edges` - The dependency graph mapping node indices to their dependencies.
///
/// ## Returns
/// A sorted list of node indices, or a `CycleError` if a cycle is detected.
pub(super) fn topological_sort(
    nodes: &[Node],
    edges: &FxHashMap<usize, FxHashSet<usize>>,
) -> Result<Vec<usize>, CycleError> {
    // Compute the number of dependencies for each node
    let mut remaining_deps: FxHashMap<usize, usize> = FxHashMap::default();
    for node in nodes {
        let count = edges.get(&node.index).map(|deps| deps.len()).unwrap_or(0);
        remaining_deps.insert(node.index, count);
    }

    // Build a mapping for each node to which nodes depend on it
    let mut node_dependents: FxHashMap<usize, FxHashSet<usize>> = FxHashMap::default();
    for (&node, deps) in edges {
        for &dep in deps {
            node_dependents.entry(dep).or_default().insert(node);
        }
    }

    // Queue of nodes with no dependencies
    let mut queue: VecDeque<usize> = remaining_deps
        .iter()
        .filter_map(|(&node_index, &count)| (count == 0).then_some(node_index))
        .collect();

    // Use Kahn's algorithm to sort the nodes
    let mut sorted = Vec::with_capacity(nodes.len());
    while let Some(node_index) = queue.pop_front() {
        // Add the node to the sorted list
        sorted.push(node_index);

        // For each node that depends on the current node, decrement its remaining dependency count
        for &dependent in node_dependents.get(&node_index).into_iter().flatten() {
            if let Some(count) = remaining_deps.get_mut(&dependent) {
                *count -= 1;
                if *count == 0 {
                    queue.push_back(dependent);
                }
            }
        }
    }

    // If not all nodes were sorted, the graph contains a cycle
    if sorted.len() != nodes.len() {
        return Err(CycleError);
    }
    Ok(sorted)
}
