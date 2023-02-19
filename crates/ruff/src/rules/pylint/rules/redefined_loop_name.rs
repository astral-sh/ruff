use crate::ast::helpers::find_names;
use crate::ast::types::{Node, Range};
use crate::ast::visitor;
use crate::ast::visitor::Visitor;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::source_code::Locator;
use crate::violation::Violation;
use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Expr, Stmt, StmtKind, Withitem};
use std::iter::zip;

define_violation!(
    /// ## What it does
    /// Checks for variables defined in `for` loops and `with` statements that get overwritten
    /// within the body, for example by another `for` loop or `with` statement or by direct
    /// assignment.
    ///
    /// ## Why is this bad?
    /// Redefinition of a loop variable inside the loop's body causes its value to differ from
    /// the original loop iteration for the remainder of the block, in a way that will likely
    /// cause bugs.
    ///
    /// In Python, unlike many other languages, `for` loops and `with` statements don't define
    /// their own scopes. Therefore, a nested loop that uses the same target variable name as
    /// an outer loop will reuse the same actual variable, and the value from the last
    /// iteration will "leak out" into the remainder of the enclosing loop.
    ///
    /// While this mistake is easy to spot in small examples, it can be hidden in larger
    /// blocks of code where the definition and redefinition of the variable may not be
    /// visible at the same time.
    ///
    /// ## Example
    /// ```python
    /// for i in range(10):
    ///     i = 9
    ///     print(i)  # prints 9 every iteration
    ///
    /// for i in range(10):
    ///     for i in range(10):  # original value overwritten
    ///         pass
    ///     print(i)  # also prints 9 every iteration
    ///
    /// with path1.open() as f:
    ///     with path2.open() as f:
    ///         f = path2.open()
    ///     print(f.readline())  # prints a line from path2
    /// ```
    pub struct RedefinedLoopName {
        pub name: String,
    }
);
impl Violation for RedefinedLoopName {
    #[derive_message_formats]
    fn message(&self) -> String {
        let RedefinedLoopName { name } = self;
        format!("For loop or with statement variable `{name}` overwritten in body")
    }
}

struct InnerForWithAssignNamesVisitor<'a> {
    locator: &'a Locator<'a>,
    name_ranges: Vec<Range>,
}

impl<'a, 'b> Visitor<'b> for InnerForWithAssignNamesVisitor<'_>
where
    'b: 'a,
{
    fn visit_stmt(&mut self, stmt: &'b Stmt) {
        match &stmt.node {
            // For and async for.
            StmtKind::For { target, .. } | StmtKind::AsyncFor { target, .. } => {
                self.name_ranges
                    .extend(name_ranges_from_expr(target, self.locator));
            }
            // With.
            StmtKind::With { items, .. } => {
                self.name_ranges
                    .extend(name_ranges_from_with_items(items, self.locator));
            }
            // Assignment, augmented assignment, and annotated assignment.
            StmtKind::Assign { targets, .. } => {
                self.name_ranges
                    .extend(name_ranges_from_assign_targets(targets, self.locator));
            }
            StmtKind::AugAssign { target, .. } | StmtKind::AnnAssign { target, .. } => {
                self.name_ranges
                    .extend(name_ranges_from_expr(target, self.locator));
            }
            _ => {}
        }
        visitor::walk_stmt(self, stmt);
    }
}

fn name_ranges_from_expr<'a, U>(target: &'a Expr<U>, locator: &Locator) -> Vec<Range> {
    find_names(target, locator)
}

fn name_ranges_from_with_items<'a, U>(items: &'a [Withitem<U>], locator: &Locator) -> Vec<Range> {
    items
        .iter()
        .filter_map(|item| {
            item.optional_vars
                .as_ref()
                .map(|expr| find_names(&**expr, locator))
        })
        .flatten()
        .collect()
}

fn name_ranges_from_assign_targets<'a, U>(targets: &'a [Expr<U>], locator: &Locator) -> Vec<Range> {
    targets
        .iter()
        .flat_map(|target| find_names(target, locator))
        .collect()
}

/// PLW2901
pub fn redefined_loop_name<'a, 'b>(checker: &'a mut Checker<'b>, node: &Node<'b>)
where
    'b: 'a,
{
    let (outer_name_ranges, inner_name_ranges) = match node {
        Node::Stmt(stmt) => match &stmt.node {
            // With.
            StmtKind::With { items, body, .. } => {
                let name_ranges = name_ranges_from_with_items(items, checker.locator);
                let mut visitor = InnerForWithAssignNamesVisitor {
                    locator: checker.locator,
                    name_ranges: vec![],
                };
                for stmt in body {
                    visitor.visit_stmt(stmt);
                }
                (name_ranges, visitor.name_ranges)
            }
            // For and async for.
            StmtKind::For {
                target,
                body,
                iter: _,
                orelse: _,
                ..
            }
            | StmtKind::AsyncFor {
                target,
                body,
                iter: _,
                orelse: _,
                ..
            } => {
                let name_ranges = name_ranges_from_expr(target, checker.locator);
                let mut visitor = InnerForWithAssignNamesVisitor {
                    locator: checker.locator,
                    name_ranges: vec![],
                };
                for stmt in body {
                    visitor.visit_stmt(stmt);
                }
                (name_ranges, visitor.name_ranges)
            }
            _ => panic!(
                "redefined_loop_name called on Statement that is not a With, For, or AsyncFor"
            ),
        },
        Node::Expr(_) => panic!("redefined_loop_name called on Node that is not a Statement"),
    };

    let outer_names: Vec<&str> = outer_name_ranges
        .iter()
        .map(|range| checker.locator.slice(range))
        // Ignore dummy variables.
        .filter(|name| !checker.settings.dummy_variable_rgx.is_match(name))
        .collect();
    let inner_names: Vec<&str> = inner_name_ranges
        .iter()
        .map(|range| checker.locator.slice(range))
        .collect();

    for outer_name in &outer_names {
        for (inner_range, inner_name) in zip(&inner_name_ranges, &inner_names) {
            if inner_name.eq(outer_name) {
                checker.diagnostics.push(Diagnostic::new(
                    RedefinedLoopName {
                        name: (*inner_name).to_string(),
                    },
                    *inner_range,
                ));
            }
        }
    }
}
