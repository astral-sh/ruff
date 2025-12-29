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

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use ruff_python_parser::parse_module;

    use super::{CycleError, Dependencies, collect_dependencies, nodes_from_suite};

    /// Test that CycleError displays correctly.
    #[test]
    fn cycle_error_display() {
        let err = CycleError {
            cycle: vec![0, 1, 2],
        };
        let display = format!("{}", err);

        assert!(display.contains("Cycle detected"));
        assert!(display.contains("0"));
        assert!(display.contains("1"));
        assert!(display.contains("2"));
    }

    /// Test that CycleError implements std::error::Error.
    #[test]
    fn cycle_error_is_error() {
        let err = CycleError { cycle: vec![0] };
        let _: &dyn std::error::Error = &err;
    }

    /// Test that functions with no external references have no dependencies.
    #[test]
    fn collect_dependencies_none() -> Result<()> {
        let source = r#"
def foo():
    return 1

def bar():
    return 2
"#;
        let parsed = parse_module(source)?;
        let nodes = nodes_from_suite(parsed.suite());

        assert_eq!(collect_dependencies(&nodes[0], &nodes).len(), 0);
        assert_eq!(collect_dependencies(&nodes[1], &nodes).len(), 0);
        Ok(())
    }

    /// Test that a simple single dependency is collected.
    #[test]
    fn collect_dependencies_simple() -> Result<()> {
        let source = r#"
def foo():
    return 1

def bar():
    return foo()
"#;
        let parsed = parse_module(source)?;
        let nodes = nodes_from_suite(parsed.suite());
        let deps = collect_dependencies(&nodes[1], &nodes);

        assert_eq!(deps, vec![0]);
        Ok(())
    }

    /// Test that multiple dependencies are collected.
    #[test]
    fn collect_dependencies_multiple() -> Result<()> {
        let source = r#"
def foo():
    return 1

def bar():
    return 2

def baz():
    return foo() + bar()
"#;
        let parsed = parse_module(source)?;
        let nodes = nodes_from_suite(parsed.suite());
        let deps = collect_dependencies(&nodes[2], &nodes);

        assert_eq!(deps.len(), 2);
        assert!(deps.contains(&0));
        assert!(deps.contains(&1));
        Ok(())
    }

    /// Test that self-referential calls (recursion) are filtered out.
    #[test]
    fn collect_dependencies_self_reference() -> Result<()> {
        let source = r#"
def factorial(n):
    return n * factorial(n - 1)
"#;
        let parsed = parse_module(source)?;
        let nodes = nodes_from_suite(parsed.suite());
        let deps = collect_dependencies(&nodes[0], &nodes);

        assert_eq!(deps.len(), 0);
        Ok(())
    }

    /// Test that non-self attribute access doesn't create dependencies.
    #[test]
    fn collect_dependencies_non_self_attribute() -> Result<()> {
        let source = r#"
def foo():
    obj = SomeClass()
    return obj.method()
"#;
        let parsed = parse_module(source)?;
        let nodes = nodes_from_suite(parsed.suite());
        let deps = collect_dependencies(&nodes[0], &nodes);

        assert_eq!(deps.len(), 0);
        Ok(())
    }

    /// Test that dependencies between classes are collected.
    #[test]
    fn collect_dependencies_between_classes() -> Result<()> {
        let source = r#"
class Base:
    pass

class Derived:
    base = Base()
"#;
        let parsed = parse_module(source)?;
        let nodes = nodes_from_suite(parsed.suite());
        let deps = collect_dependencies(&nodes[1], &nodes);

        assert_eq!(deps, vec![0]);
        Ok(())
    }

    /// Test that nested function calls are tracked (transitivity).
    #[test]
    fn collect_dependencies_nested_calls() -> Result<()> {
        let source = r#"
def a():
    return 1

def b():
    return a()

def c():
    return b()
"#;
        let parsed = parse_module(source)?;
        let nodes = nodes_from_suite(parsed.suite());

        assert_eq!(collect_dependencies(&nodes[1], &nodes), vec![0]);
        assert_eq!(collect_dependencies(&nodes[2], &nodes), vec![1]);
        Ok(())
    }

    /// Test that dependencies in various expression contexts are collected.
    #[test]
    fn collect_dependencies_various_contexts() -> Result<()> {
        let source = r#"
def foo():
    return [1, 2]

def bar():
    return foo()[0] + len(foo())
"#;
        let parsed = parse_module(source)?;
        let nodes = nodes_from_suite(parsed.suite());
        let deps = collect_dependencies(&nodes[1], &nodes);

        assert_eq!(deps, vec![0]);
        Ok(())
    }

    /// Test that nodes with no dependencies maintain their original order.
    #[test]
    fn dependency_order_preserved() -> Result<()> {
        let source = r#"
def foo():
    pass

def bar():
    pass
"#;
        let parsed = parse_module(source)?;
        let nodes = nodes_from_suite(parsed.suite());
        let deps = Dependencies::from_nodes(&nodes);
        let order = deps.dependency_order(&nodes, false)?;

        assert_eq!(order, vec![0, 1]);
        Ok(())
    }

    /// Test that a simple reordering works correctly.
    #[test]
    fn dependency_order_simple_reorder() -> Result<()> {
        let source = r#"
def bar():
    return foo()

def foo():
    return 1
"#;
        let parsed = parse_module(source)?;
        let nodes = nodes_from_suite(parsed.suite());
        let deps = Dependencies::from_nodes(&nodes);
        let order = deps.dependency_order(&nodes, false)?;

        assert_eq!(order, vec![1, 0]);
        Ok(())
    }

    /// Test that a dependency chain is ordered correctly.
    #[test]
    fn dependency_order_chain() -> Result<()> {
        let source = r#"
def baz():
    return bar()

def bar():
    return foo()

def foo():
    return 1
"#;
        let parsed = parse_module(source)?;
        let nodes = nodes_from_suite(parsed.suite());
        let deps = Dependencies::from_nodes(&nodes);
        let order = deps.dependency_order(&nodes, false)?;

        assert_eq!(order, vec![2, 1, 0]);
        Ok(())
    }

    /// Test that non-movable statements stay in their original positions.
    #[test]
    fn dependency_order_non_movable_preserved() -> Result<()> {
        let source = r#"
x = 1

def foo():
    return x

def bar():
    return foo()
"#;
        let parsed = parse_module(source)?;
        let nodes = nodes_from_suite(parsed.suite());
        let deps = Dependencies::from_nodes(&nodes);
        let order = deps.dependency_order(&nodes, false)?;

        assert_eq!(order, vec![0, 1, 2]);
        Ok(())
    }

    /// Test that cycle detection between two functions works correctly.
    #[test]
    fn dependency_order_simple_cycle() -> Result<()> {
        let source = r#"
def foo():
    return bar()

def bar():
    return foo()
"#;
        let parsed = parse_module(source)?;
        let nodes = nodes_from_suite(parsed.suite());
        let deps = Dependencies::from_nodes(&nodes);
        let result = deps.dependency_order(&nodes, false);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.cycle, vec![0, 1]);
        Ok(())
    }

    /// Test that cycle detection with three functions works correctly.
    #[test]
    fn dependency_order_three_way_cycle() -> Result<()> {
        let source = r#"
def a():
    return b()

def b():
    return c()

def c():
    return a()
"#;
        let parsed = parse_module(source)?;
        let nodes = nodes_from_suite(parsed.suite());
        let deps = Dependencies::from_nodes(&nodes);
        let result = deps.dependency_order(&nodes, false);

        assert!(result.is_err());
        assert!(!result.unwrap_err().cycle.is_empty());
        Ok(())
    }

    /// Test that narrative mode reverses movable nodes while keeping non-movable nodes in place.
    #[test]
    fn dependency_order_narrative_simple() -> Result<()> {
        let source = r#"
def baz():
    return bar()

def bar():
    return foo()

def foo():
    return 1
"#;
        let parsed = parse_module(source)?;
        let nodes = nodes_from_suite(parsed.suite());
        let deps = Dependencies::from_nodes(&nodes);
        let order = deps.dependency_order(&nodes, true)?;

        assert_eq!(order, vec![0, 1, 2]);
        Ok(())
    }

    /// Test that narrative mode with non-movable statements interspersed works correctly.
    #[test]
    fn dependency_order_narrative_with_non_movable() -> Result<()> {
        let source = r#"
x = 1

def foo():
    return x

y = 2

def bar():
    return foo()
"#;
        let parsed = parse_module(source)?;
        let nodes = nodes_from_suite(parsed.suite());
        let deps = Dependencies::from_nodes(&nodes);
        let order = deps.dependency_order(&nodes, true)?;

        assert_eq!(order.len(), 4);
        assert_eq!(order[0], 0);
        assert_eq!(order[1], 3);
        assert_eq!(order[2], 2);
        assert_eq!(order[3], 1);
        Ok(())
    }

    /// Test that complex diamond dependency graphs are ordered correctly.
    #[test]
    fn dependency_order_diamond() -> Result<()> {
        let source = r#"
def d():
    return c() + b()

def c():
    return a()

def b():
    return a()

def a():
    return 1
"#;
        let parsed = parse_module(source)?;
        let nodes = nodes_from_suite(parsed.suite());
        let deps = Dependencies::from_nodes(&nodes);
        let order = deps.dependency_order(&nodes, false)?;

        assert_eq!(order[0], 3);
        assert_eq!(order[3], 0);
        assert!([1, 2].contains(&order[1]));
        assert!([1, 2].contains(&order[2]));
        assert_ne!(order[1], order[2]);
        Ok(())
    }

    /// Test that ordering with interleaved non-movable and movable statements works correctly.
    #[test]
    fn dependency_order_interleaved() -> Result<()> {
        let source = r#"
x = 1

def foo():
    return x

y = 2

def bar():
    return y

z = 3
"#;
        let parsed = parse_module(source)?;
        let nodes = nodes_from_suite(parsed.suite());
        let deps = Dependencies::from_nodes(&nodes);
        let order = deps.dependency_order(&nodes, false)?;

        assert_eq!(order[0], 0);
        assert_eq!(order[2], 2);
        assert_eq!(order[4], 4);
        Ok(())
    }

    /// Test that a single-node graph works correctly.
    #[test]
    fn dependency_order_single_node() -> Result<()> {
        let source = r#"
def foo():
    pass
"#;
        let parsed = parse_module(source)?;
        let nodes = nodes_from_suite(parsed.suite());
        let deps = Dependencies::from_nodes(&nodes);
        let order = deps.dependency_order(&nodes, false)?;

        assert_eq!(order, vec![0]);
        Ok(())
    }

    /// Test that an empty source file produces no nodes.
    #[test]
    fn nodes_from_suite_empty() -> Result<()> {
        let parsed = parse_module("")?;
        let nodes = nodes_from_suite(parsed.suite());
        assert_eq!(nodes.len(), 0);
        Ok(())
    }

    /// Test that function definitions are correctly extracted as movable nodes.
    #[test]
    fn nodes_from_suite_functions() -> Result<()> {
        let source = r#"
def foo():
    pass

def bar():
    pass
"#;
        let parsed = parse_module(source)?;
        let nodes = nodes_from_suite(parsed.suite());

        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0].name.map(|n| n.as_str()), Some("foo"));
        assert!(nodes[0].movable);
        assert_eq!(nodes[1].name.map(|n| n.as_str()), Some("bar"));
        assert!(nodes[1].movable);
        Ok(())
    }

    /// Test that class definitions are correctly extracted as movable nodes.
    #[test]
    fn nodes_from_suite_classes() -> Result<()> {
        let source = r#"
class Foo:
    pass

class Bar:
    pass
"#;
        let parsed = parse_module(source)?;
        let nodes = nodes_from_suite(parsed.suite());

        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0].name.map(|n| n.as_str()), Some("Foo"));
        assert!(nodes[0].movable);
        assert_eq!(nodes[1].name.map(|n| n.as_str()), Some("Bar"));
        assert!(nodes[1].movable);
        Ok(())
    }

    /// Test that non-function/class statements are non-movable.
    #[test]
    fn nodes_from_suite_non_movable() -> Result<()> {
        let source = r#"
x = 1
y = 2

def foo():
    pass
"#;
        let parsed = parse_module(source)?;
        let nodes = nodes_from_suite(parsed.suite());

        assert_eq!(nodes.len(), 3);
        assert_eq!(nodes[0].name, None);
        assert!(!nodes[0].movable);
        assert_eq!(nodes[1].name, None);
        assert!(!nodes[1].movable);
        assert_eq!(nodes[2].name.map(|n| n.as_str()), Some("foo"));
        assert!(nodes[2].movable);
        Ok(())
    }

    /// Test mixed classes and functions are all movable.
    #[test]
    fn nodes_from_suite_mixed() -> Result<()> {
        let source = r#"
class Foo:
    pass

def bar():
    pass

class Baz:
    pass
"#;
        let parsed = parse_module(source)?;
        let nodes = nodes_from_suite(parsed.suite());

        assert_eq!(nodes.len(), 3);
        assert!(nodes.iter().all(|n| n.movable));
        assert_eq!(nodes[0].name.map(|n| n.as_str()), Some("Foo"));
        assert_eq!(nodes[1].name.map(|n| n.as_str()), Some("bar"));
        assert_eq!(nodes[2].name.map(|n| n.as_str()), Some("Baz"));
        Ok(())
    }
}
