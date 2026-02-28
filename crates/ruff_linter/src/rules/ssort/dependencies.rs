use std::fmt;

use rustc_hash::FxHashMap;

use ruff_python_ast::name::Name;
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::{Expr, InterpolatedStringElement, Stmt};

/// An error indicating that a cycle was detected in the dependency graph.
#[derive(Debug)]
pub(super) struct CycleError {
    /// The indices of nodes involved in the cycle, in call order.
    pub(crate) cycle: Vec<usize>,
}

/// Allows `CycleError` to be treated as a standard error.
impl std::error::Error for CycleError {}

/// Allows `CycleError` to be formatted for display.
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

/// A visitor that collects all variable names referenced by a statement.
#[derive(Default)]
struct DependencyVisitor<'a> {
    /// The names referenced by the statement.
    names: Vec<&'a Name>,

    /// A mapping from names to their corresponding node indices.
    name_to_index: FxHashMap<&'a Name, usize>,

    /// The current depth of function definitions during traversal.
    function_depth: usize,

    /// The current depth of class definitions during traversal.
    class_depth: usize,
}

impl<'a> DependencyVisitor<'a> {
    /// Create a new `DependencyVisitor` with the given name-to-index mapping.
    ///
    /// ## Arguments
    /// * `nodes` - The list of all nodes in the AST.
    pub(crate) fn new(nodes: &[Node<'a>]) -> Self {
        let name_to_index: FxHashMap<_, _> = nodes
            .iter()
            .filter_map(|node| node.name.map(|name| (name, node.index)))
            .collect();

        Self {
            names: Vec::new(),
            name_to_index,
            function_depth: 0,
            class_depth: 0,
        }
    }
}

/// Allows `DependencyVisitor` to be used as a visitor for AST nodes.
impl<'a> Visitor<'a> for DependencyVisitor<'a> {
    /// Visit a statement node during AST traversal.
    ///
    /// ## Arguments
    /// * `stmt` - The statement node to visit.
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            // Only traverse class definitions to capture class-level references
            Stmt::ClassDef(_) => {
                self.class_depth += 1;
                ruff_python_ast::visitor::walk_stmt(self, stmt);
                self.class_depth -= 1;
            }

            // Only traverse top-level function definitions to capture global references
            Stmt::FunctionDef(_) => {
                if self.function_depth == 0 && self.class_depth == 0 {
                    self.function_depth += 1;
                    ruff_python_ast::visitor::walk_stmt(self, stmt);
                    self.function_depth -= 1;
                }
            }

            // Everything else: traverse normally
            _ => ruff_python_ast::visitor::walk_stmt(self, stmt),
        }
    }

    /// Visit an expression node during AST traversal.
    ///
    /// ## Arguments
    /// * `expr` - The expression node to visit.
    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr {
            // Simple variable reference: `foo`
            Expr::Name(name) => {
                self.names.push(&name.id);
            }

            // Attribute access: `self.foo` or `obj.foo`
            Expr::Attribute(attr) if matches!(&*attr.value, Expr::Name(n) if n.id.as_str() == "self") =>
            {
                self.names.push(&attr.attr.id);
            }

            // Attribute access: `obj.foo`
            Expr::Attribute(attr) => {
                self.visit_expr(&attr.value);
            }

            // Function/method call: `func(arg1, arg2, ...)`
            Expr::Call(call) => {
                self.visit_expr(&call.func);
                call.arguments
                    .args
                    .iter()
                    .for_each(|arg| self.visit_expr(arg));
                call.arguments
                    .keywords
                    .iter()
                    .for_each(|kw| self.visit_expr(&kw.value));
            }

            // f-strings: `f"Hello {name}"`
            Expr::FString(expr_fstring) => {
                for element in expr_fstring.value.elements() {
                    if let InterpolatedStringElement::Interpolation(interp) = element {
                        self.visit_expr(&interp.expression);
                    }
                }
            }

            // Binary operations: `a + b`
            Expr::BinOp(binop) => {
                self.visit_expr(&binop.left);
                self.visit_expr(&binop.right);
            }

            // Boolean operations: `a and b`, `a or b`
            Expr::BoolOp(boolop) => {
                boolop.values.iter().for_each(|v| self.visit_expr(v));
            }

            // Conditional expression: `a if cond else b`
            Expr::If(ifexp) => {
                self.visit_expr(&ifexp.test);
                self.visit_expr(&ifexp.body);
                self.visit_expr(&ifexp.orelse);
            }

            // List literal: `[elem1, elem2, ...]`
            Expr::List(list) => list.elts.iter().for_each(|elt| self.visit_expr(elt)),

            // Tuple literal: `(elem1, elem2, ...)
            Expr::Tuple(list) => list.elts.iter().for_each(|elt| self.visit_expr(elt)),

            // Set literal: `{elem1, elem2, ...}`
            Expr::Set(set) => set.elts.iter().for_each(|elt| self.visit_expr(elt)),

            // Dictionary literal: `{key: value, ...}`
            Expr::Dict(dict) => {
                for item in &dict.items {
                    if let Some(key) = &item.key {
                        self.visit_expr(key);
                    }
                    self.visit_expr(&item.value);
                }
            }

            // Generator expressions: `(x for x in iterable if cond)`
            Expr::Generator(generator) => {
                self.visit_expr(&generator.elt);
                for comp in &generator.generators {
                    self.visit_expr(&comp.iter);
                    comp.ifs.iter().for_each(|if_| self.visit_expr(if_));
                }
            }

            // Everything else: leaf expressions or unsupported types
            _ => {}
        }
    }
}

/// Collect the indices of nodes that the given node depends on.
///
/// ## Arguments
/// * `node` - The node for which to collect dependencies.
/// * `visitor` - A reusable dependency visitor to traverse the node's statement.
///
/// ## Returns
/// A vector of node indices that this node depends on.
fn collect_dependencies<'a>(node: &'a Node, visitor: &mut DependencyVisitor<'a>) -> Vec<usize> {
    // Reset the visitor's state
    visitor.names.clear();

    // Visit the statement to collect referenced names
    visitor.visit_stmt(node.stmt);

    // Deduplicate the collected names
    visitor.names.sort_unstable();
    visitor.names.dedup();

    // Map referenced names to node indices, skipping unknown names and self-references
    visitor
        .names
        .iter()
        .filter_map(|name| visitor.name_to_index.get(name).copied())
        .filter(|&idx| idx != node.index)
        .collect()
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
        let mut visitor = DependencyVisitor::new(nodes);
        Self {
            dependencies: nodes
                .iter()
                .map(|node| collect_dependencies(node, &mut visitor))
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
    /// A vector of node indices in dependency-respecting order, or a `CycleError` if a cycle is
    /// detected.
    pub(super) fn dependency_order(
        &self,
        nodes: &[Node],
        narrative_order: bool,
    ) -> Result<Vec<usize>, CycleError> {
        fn emit_deps(
            idx: usize,
            deps: &[Vec<usize>],
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
                emit_deps(dep, deps, emitted, visiting, stack, result)?;
            }

            // Mark this node as no longer being visited and emit it
            stack.pop();
            visiting[idx] = false;
            emitted[idx] = true;
            result.push(idx);
            Ok(())
        }

        // Tracks which nodes have already been emitted to avoid duplicates
        let mut emitted = vec![false; nodes.len()];

        // Tracks which nodes are currently being visited to detect cycles
        let mut visiting = vec![false; nodes.len()];

        // Tracks the current stack of nodes being visited for cycle reporting
        let mut stack = Vec::new();

        // Iterate over all nodes and return the dependency-respecting order
        let mut result = Vec::with_capacity(nodes.len());
        for idx in 0..nodes.len() {
            emit_deps(
                idx,
                &self.dependencies,
                &mut emitted,
                &mut visiting,
                &mut stack,
                &mut result,
            )?;
        }
        if narrative_order {
            // Collect all movable nodes in reverse order
            let movable: Vec<usize> = result
                .iter()
                .copied()
                .filter(|&idx| nodes[idx].movable)
                .collect();
            let mut movable_iter = movable.into_iter().rev();

            // Reconstruct the result, replacing movable nodes with their reversed counterparts
            result = result
                .into_iter()
                .map(|idx| {
                    if nodes[idx].movable {
                        movable_iter.next().unwrap()
                    } else {
                        idx
                    }
                })
                .collect();
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
    use ruff_python_ast::name::Name;
    use ruff_python_parser::parse_module;

    use super::{
        CycleError, Dependencies, DependencyVisitor, collect_dependencies, nodes_from_suite,
    };

    /// Test that `CycleError` displays correctly.
    #[test]
    fn cycle_error_display() {
        let err = CycleError {
            cycle: vec![0, 1, 2],
        };
        let display = format!("{err}");

        assert!(display.contains("Cycle detected"));
        assert!(display.contains('0'));
        assert!(display.contains('1'));
        assert!(display.contains('2'));
    }

    /// Test that `CycleError` implements `std::error::Error`.
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
        let mut visitor = DependencyVisitor::new(&nodes);

        assert_eq!(collect_dependencies(&nodes[0], &mut visitor).len(), 0);
        assert_eq!(collect_dependencies(&nodes[1], &mut visitor).len(), 0);
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
        let mut visitor = DependencyVisitor::new(&nodes);
        let deps = collect_dependencies(&nodes[1], &mut visitor);

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
        let mut visitor = DependencyVisitor::new(&nodes);
        let deps = collect_dependencies(&nodes[2], &mut visitor);

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
        let mut visitor = DependencyVisitor::new(&nodes);
        let deps = collect_dependencies(&nodes[0], &mut visitor);

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
        let mut visitor = DependencyVisitor::new(&nodes);
        let deps = collect_dependencies(&nodes[0], &mut visitor);

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
        let mut visitor = DependencyVisitor::new(&nodes);
        let deps = collect_dependencies(&nodes[1], &mut visitor);

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
        let mut visitor = DependencyVisitor::new(&nodes);

        assert_eq!(collect_dependencies(&nodes[1], &mut visitor), vec![0]);
        assert_eq!(collect_dependencies(&nodes[2], &mut visitor), vec![1]);
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
        let mut visitor = DependencyVisitor::new(&nodes);
        let deps = collect_dependencies(&nodes[1], &mut visitor);

        assert_eq!(deps, vec![0]);
        Ok(())
    }

    /// Test that dependencies in f-strings are collected.
    #[test]
    fn collect_dependencies_fstring() -> Result<()> {
        let source = r#"
def foo():
    return "world"

def bar():
    return f"Hello {foo()}"
"#;
        let parsed = parse_module(source)?;
        let nodes = nodes_from_suite(parsed.suite());
        let mut visitor = DependencyVisitor::new(&nodes);
        let deps = collect_dependencies(&nodes[1], &mut visitor);

        assert_eq!(deps, vec![0]);
        Ok(())
    }

    /// Test that dependencies in f-strings with multiple interpolations are collected.
    #[test]
    fn collect_dependencies_fstring_multiple() -> Result<()> {
        let source = r#"
def foo():
    return "hello"

def bar():
    return "world"

def baz():
    return f"{foo()} {bar()}"
"#;
        let parsed = parse_module(source)?;
        let nodes = nodes_from_suite(parsed.suite());
        let mut visitor = DependencyVisitor::new(&nodes);
        let deps = collect_dependencies(&nodes[2], &mut visitor);

        assert_eq!(deps.len(), 2);
        assert!(deps.contains(&0));
        assert!(deps.contains(&1));
        Ok(())
    }

    /// Test that dependencies in binary operations are collected.
    #[test]
    fn collect_dependencies_binop() -> Result<()> {
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
        let mut visitor = DependencyVisitor::new(&nodes);
        let deps = collect_dependencies(&nodes[2], &mut visitor);

        assert_eq!(deps.len(), 2);
        assert!(deps.contains(&0));
        assert!(deps.contains(&1));
        Ok(())
    }

    /// Test that dependencies in boolean operations are collected.
    #[test]
    fn collect_dependencies_boolop() -> Result<()> {
        let source = r#"
def foo():
    return True

def bar():
    return False

def baz():
    return foo() and bar()
"#;
        let parsed = parse_module(source)?;
        let nodes = nodes_from_suite(parsed.suite());
        let mut visitor = DependencyVisitor::new(&nodes);
        let deps = collect_dependencies(&nodes[2], &mut visitor);

        assert_eq!(deps.len(), 2);
        assert!(deps.contains(&0));
        assert!(deps.contains(&1));
        Ok(())
    }

    /// Test that dependencies in conditional expressions are collected.
    #[test]
    fn collect_dependencies_if_expr() -> Result<()> {
        let source = r#"
def foo():
    return True

def bar():
    return 1

def baz():
    return 2

def qux():
    return bar() if foo() else baz()
"#;
        let parsed = parse_module(source)?;
        let nodes = nodes_from_suite(parsed.suite());
        let mut visitor = DependencyVisitor::new(&nodes);
        let deps = collect_dependencies(&nodes[3], &mut visitor);

        assert_eq!(deps.len(), 3);
        assert!(deps.contains(&0));
        assert!(deps.contains(&1));
        assert!(deps.contains(&2));
        Ok(())
    }

    /// Test that dependencies in list literals are collected.
    #[test]
    fn collect_dependencies_list() -> Result<()> {
        let source = r#"
def foo():
    return 1

def bar():
    return 2

def baz():
    return [foo(), bar()]
"#;
        let parsed = parse_module(source)?;
        let nodes = nodes_from_suite(parsed.suite());
        let mut visitor = DependencyVisitor::new(&nodes);
        let deps = collect_dependencies(&nodes[2], &mut visitor);

        assert_eq!(deps.len(), 2);
        assert!(deps.contains(&0));
        assert!(deps.contains(&1));
        Ok(())
    }

    /// Test that dependencies in tuple literals are collected.
    #[test]
    fn collect_dependencies_tuple() -> Result<()> {
        let source = r#"
def foo():
    return 1

def bar():
    return 2

def baz():
    return (foo(), bar())
"#;
        let parsed = parse_module(source)?;
        let nodes = nodes_from_suite(parsed.suite());
        let mut visitor = DependencyVisitor::new(&nodes);
        let deps = collect_dependencies(&nodes[2], &mut visitor);

        assert_eq!(deps.len(), 2);
        assert!(deps.contains(&0));
        assert!(deps.contains(&1));
        Ok(())
    }

    /// Test that dependencies in set literals are collected.
    #[test]
    fn collect_dependencies_set() -> Result<()> {
        let source = r#"
def foo():
    return 1

def bar():
    return 2

def baz():
    return {foo(), bar()}
"#;
        let parsed = parse_module(source)?;
        let nodes = nodes_from_suite(parsed.suite());
        let mut visitor = DependencyVisitor::new(&nodes);
        let deps = collect_dependencies(&nodes[2], &mut visitor);

        assert_eq!(deps.len(), 2);
        assert!(deps.contains(&0));
        assert!(deps.contains(&1));
        Ok(())
    }

    /// Test that dependencies in dictionary literals are collected.
    #[test]
    fn collect_dependencies_dict() -> Result<()> {
        let source = r#"
def foo():
    return "key"

def bar():
    return "value"

def baz():
    return {foo(): bar()}
"#;
        let parsed = parse_module(source)?;
        let nodes = nodes_from_suite(parsed.suite());
        let mut visitor = DependencyVisitor::new(&nodes);
        let deps = collect_dependencies(&nodes[2], &mut visitor);

        assert_eq!(deps.len(), 2);
        assert!(deps.contains(&0));
        assert!(deps.contains(&1));
        Ok(())
    }

    /// Test that dependencies in dictionary literals with only values are collected.
    #[test]
    fn collect_dependencies_dict_values_only() -> Result<()> {
        let source = r#"
def foo():
    return 1

def bar():
    return {"key": foo()}
"#;
        let parsed = parse_module(source)?;
        let nodes = nodes_from_suite(parsed.suite());
        let mut visitor = DependencyVisitor::new(&nodes);
        let deps = collect_dependencies(&nodes[1], &mut visitor);

        assert_eq!(deps, vec![0]);
        Ok(())
    }

    /// Test that dependencies in generator expressions are collected.
    #[test]
    fn collect_dependencies_generator() -> Result<()> {
        let source = r#"
def foo():
    return [1, 2, 3]

def bar():
    return (x * 2 for x in foo())
"#;
        let parsed = parse_module(source)?;
        let nodes = nodes_from_suite(parsed.suite());
        let mut visitor = DependencyVisitor::new(&nodes);
        let deps = collect_dependencies(&nodes[1], &mut visitor);

        assert_eq!(deps, vec![0]);
        Ok(())
    }

    /// Test that dependencies in generator expressions with filters are collected.
    #[test]
    fn collect_dependencies_generator_with_filter() -> Result<()> {
        let source = r#"
def foo():
    return [1, 2, 3]

def bar(x):
    return x > 1

def baz():
    return (x for x in foo() if bar(x))
"#;
        let parsed = parse_module(source)?;
        let nodes = nodes_from_suite(parsed.suite());
        let mut visitor = DependencyVisitor::new(&nodes);
        let deps = collect_dependencies(&nodes[2], &mut visitor);

        assert_eq!(deps.len(), 2);
        assert!(deps.contains(&0));
        assert!(deps.contains(&1));
        Ok(())
    }

    /// Test that dependencies in complex nested expressions are collected.
    #[test]
    fn collect_dependencies_nested_complex() -> Result<()> {
        let source = r#"
def foo():
    return 1

def bar():
    return 2

def baz():
    return 3

def qux():
    return [foo() if bar() else baz()]
"#;
        let parsed = parse_module(source)?;
        let nodes = nodes_from_suite(parsed.suite());
        let mut visitor = DependencyVisitor::new(&nodes);
        let deps = collect_dependencies(&nodes[3], &mut visitor);

        assert_eq!(deps.len(), 3);
        assert!(deps.contains(&0));
        assert!(deps.contains(&1));
        assert!(deps.contains(&2));
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
        assert_eq!(nodes[0].name.map(Name::as_str), Some("foo"));
        assert!(nodes[0].movable);
        assert_eq!(nodes[1].name.map(Name::as_str), Some("bar"));
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
        assert_eq!(nodes[0].name.map(Name::as_str), Some("Foo"));
        assert!(nodes[0].movable);
        assert_eq!(nodes[1].name.map(Name::as_str), Some("Bar"));
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
        assert_eq!(nodes[2].name.map(Name::as_str), Some("foo"));
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
        assert_eq!(nodes[0].name.map(Name::as_str), Some("Foo"));
        assert_eq!(nodes[1].name.map(Name::as_str), Some("bar"));
        assert_eq!(nodes[2].name.map(Name::as_str), Some("Baz"));
        Ok(())
    }
}
