use std::fmt;

use rustc_hash::FxHashSet;

use ruff_python_ast::name::Name;
use ruff_python_ast::visitor::{Visitor, walk_expr};
use ruff_python_ast::{Expr, Stmt};

/// An error indicating that a cycle was detected in the dependency graph.
#[derive(Debug)]
pub(super) struct CycleError {
    /// The indices of nodes involved in the cycle, in call order.
    pub(crate) cycle: Vec<usize>,
}

/// Allows CycleError to be treated as a standard error.
impl std::error::Error for CycleError {}

/// Allows CycleError to be formatted for display.
impl fmt::Display for CycleError {
    /// Format the cycle error for display.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Cycle detected involving nodes {:?}", self.cycle)
    }
}

/// A statement in the dependency graph.
#[derive(Debug)]
pub(super) struct Node<'a> {
    /// The index of the statement in the original statement list.
    pub(crate) index: usize,

    /// The name of the statement.
    pub(crate) name: Option<&'a Name>,

    /// The statement in the AST.
    pub(crate) stmt: &'a Stmt,

    /// Whether this node can be moved during sorting.
    pub(crate) movable: bool,
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
        match expr {
            // Unqualified reference, e.g. `foo()`
            Expr::Name(name) => {
                self.names.insert(&name.id);
            }

            // Instance method call, e.g. `self.foo()`
            Expr::Attribute(attr) => {
                if let Expr::Name(value) = &*attr.value {
                    if value.id.as_str() == "self" {
                        self.names.insert(&attr.attr.id);
                    }
                }
            }

            // Other expressions are not relevant for dependency collection
            _ => {}
        }
        walk_expr(self, expr);
    }
}

/// Collect the indices of nodes that the given node depends on.
///
/// ## Arguments
/// * `node` - The node for which to collect dependencies.
/// * `nodes` - The list of all nodes extracted from the module.
///
/// ## Returns
/// A vector of node indices that this node depends on.
fn collect_dependencies<'a>(node: &'a Node, nodes: &[Node<'a>]) -> Vec<usize> {
    // Collect the names referenced in the statement
    let mut visitor = DependencyVisitor {
        names: FxHashSet::default(),
    };
    visitor.visit_stmt(node.stmt);

    // For each referenced name, find the corresponding node index
    let mut dependencies = Vec::new();
    for &referenced in &visitor.names {
        for other in nodes {
            // Skip self-references
            if other.index == node.index {
                continue;
            }

            // If it matches, exit immediately as there will only be one match
            if other.name == Some(referenced) {
                dependencies.push(other.index);
                break;
            }
        }
    }
    dependencies
}

/// Holds dependencies between nodes.
pub(super) struct Dependencies {
    /// The indices of movable nodes each node depends on, indexed by the node index.
    dependencies: Vec<Vec<usize>>,
}

/// Add methods to the Dependencies struct.
impl Dependencies {
    /// Construct a `Dependencies` instance by collecting dependencies for each node.
    ///
    /// ## Arguments
    /// * `nodes` - The list of all nodes in the AST.
    ///
    /// ## Returns
    /// A `Dependencies` instance containing the collected dependencies.
    pub(super) fn from_nodes(nodes: &[Node]) -> Self {
        Self {
            dependencies: nodes
                .iter()
                .map(|node| collect_dependencies(node, nodes))
                .collect(),
        }
    }

    /// Produce a minimally reordered list of node indices that respects dependencies.
    ///
    /// This implementation uses a depth-first search (DFS) approach to ensure that all dependencies
    /// of a node are emitted before the node itself similar to a topological sort. See
    /// <https://en.wikipedia.org/wiki/Topological_sorting#Depth-first_search> for more details.
    ///
    /// ## Arguments
    /// * `nodes` - The list of all nodes in the AST.
    /// * `narrative_order` - Whether to use a narrative-oriented order for the result.
    ///
    /// ## Returns
    /// A vector of node indices in dependency-respecting order, or a CycleError if a cycle is
    /// detected.
    pub(super) fn dependency_order(
        &self,
        nodes: &[Node],
        narrative_order: bool,
    ) -> Result<Vec<usize>, CycleError> {
        // Tracks which nodes have already been emitted to avoid duplicates
        let mut emitted = vec![false; nodes.len()];

        // Tracks which nodes are currently being visited to detect cycles
        let mut visiting = vec![false; nodes.len()];

        // Tracks the current stack of nodes being visited for cycle reporting
        let mut stack = Vec::new();

        fn emit_deps(
            idx: usize,
            deps: &[Vec<usize>],
            nodes: &[Node],
            emitted: &mut [bool],
            visiting: &mut [bool],
            stack: &mut Vec<usize>,
            result: &mut Vec<usize>,
        ) -> Result<(), CycleError> {
            // If we're already visiting this node, we've detected a cycle
            if visiting[idx] {
                let pos = stack.iter().position(|&n| n == idx).unwrap();
                return Err(CycleError {
                    cycle: stack[pos..].to_vec(),
                });
            }

            // If we've already emitted this node, skip it
            if emitted[idx] {
                return Ok(());
            }

            // Temporarily mark this node as being visited to detect cycles
            visiting[idx] = true;
            stack.push(idx);

            // Recursively emit all movable dependencies first
            for &dep in &deps[idx] {
                if nodes[dep].movable {
                    emit_deps(dep, deps, nodes, emitted, visiting, stack, result)?;
                }
            }

            // Mark this node as no longer being visited and emit it
            stack.pop();
            visiting[idx] = false;
            emitted[idx] = true;
            result.push(idx);
            Ok(())
        }

        // Iterate over all nodes and return the dependency-respecting order
        let mut result = Vec::with_capacity(nodes.len());
        for idx in 0..nodes.len() {
            emit_deps(
                idx,
                &self.dependencies,
                nodes,
                &mut emitted,
                &mut visiting,
                &mut stack,
                &mut result,
            )?;
        }
        if narrative_order {
            // Collect positions of movable nodes
            let positions: Vec<usize> = result
                .iter()
                .enumerate()
                .filter_map(|(i, &idx)| nodes[idx].movable.then_some(i))
                .collect();

            // Swap movable nodes symmetrically to reverse their order
            for k in 0..positions.len() / 2 {
                let i = positions[k];
                let j = positions[positions.len() - 1 - k];
                result.swap(i, j);
            }
        }
        Ok(result)
    }
}

/// Extract all statements from a suite and convert them into nodes.
///
/// Note that only function and class definitions are marked as movable.
///
/// ## Arguments
/// * `suite` - The suite of statements to extract nodes from.
///
/// ## Returns
/// A vector of nodes representing the statements in the suite.
pub(super) fn nodes_from_suite(suite: &'_ [Stmt]) -> Vec<Node<'_>> {
    suite
        .iter()
        .enumerate()
        .map(|(index, stmt)| {
            let name = match stmt {
                Stmt::FunctionDef(def) => Some(&def.name.id),
                Stmt::ClassDef(def) => Some(&def.name.id),
                _ => None,
            };
            Node {
                index,
                name,
                stmt,
                movable: name.is_some(),
            }
        })
        .collect()
}
