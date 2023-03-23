use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;
use ruff_python_ast::visitor::{self, Visitor};
use rustpython_parser::ast::{Expr, ExprKind, Stmt, StmtKind};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for multiple usage of the generator returned from
/// `itertools.groupby()`.
///
/// ## Why is it bad?
/// Using the generator more than once will do nothing on the second usage.
/// If that data is needed later, it should be stored as a list.
///
/// ## Examples:
/// ```python
/// import itertools
///
/// for name, group in itertools.groupby(data):
///     for _ in range(5):
///         do_something_with_the_group(group)
/// ```
///
/// Use instead:
/// ```python
/// import itertools
///
/// for name, group in itertools.groupby(data):
///     values = list(group)
///     for _ in range(5):
///         do_something_with_the_group(values)
/// ```
#[violation]
pub struct ReuseOfGroupbyGenerator;

impl Violation for ReuseOfGroupbyGenerator {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Using the generator returned from `itertools.groupby()` more than once will do nothing on the second usage. Save the result to a list, if the result is needed multiple times.")
    }
}

/// A [`Visitor`] that collects all the usage of `group_name` in the body of
/// a `for` loop.
struct GroupNameFinder<'a> {
    /// Variable name for the group.
    group_name: &'a str,
    /// Number of times the `group_name` variable was seen during the visit.
    usage_count: u8,
    /// A flag indicating that the visitor is inside a nested `for` loop.
    nested: bool,
    exprs: Vec<&'a Expr>,
}

impl<'a> GroupNameFinder<'a> {
    fn new(group_name: &'a str) -> Self {
        GroupNameFinder {
            group_name,
            usage_count: 0,
            nested: false,
            exprs: Vec::new(),
        }
    }
}

impl<'a, 'b> Visitor<'b> for GroupNameFinder<'a>
where
    'b: 'a,
{
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match &stmt.node {
            StmtKind::For { body, .. } => {
                self.nested = true;
                visitor::walk_body(self, body);
                self.nested = false;
            }
            _ => visitor::walk_stmt(self, stmt),
        }
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        if let ExprKind::Name { id, .. } = &expr.node {
            if id == self.group_name {
                if self.nested {
                    // For nested loops, the count should not be checked as
                    // the variable usage could be once but the loop makes it
                    // being used multiple times.
                    self.exprs.push(expr);
                } else {
                    self.usage_count += 1;
                    if self.usage_count > 1 {
                        self.exprs.push(expr);
                    }
                }
            }
        }
        visitor::walk_expr(self, expr);
    }
}

/// B031
pub fn reuse_of_groupby_generator(
    checker: &mut Checker,
    target: &Expr,
    body: &[Stmt],
    iter: &Expr,
) {
    let ExprKind::Call { func, .. } = &iter.node else {
        return;
    };
    // Check if the function call is `itertools.groupby`
    if !checker
        .ctx
        .resolve_call_path(func)
        .map_or(false, |call_path| {
            call_path.as_slice() == ["itertools", "groupby"]
        })
    {
        return;
    }
    let ExprKind::Tuple { elts, .. } = &target.node else {
        // Ignore any `groupby()` invocation that isn't unpacked
        return;
    };
    if elts.len() != 2 {
        return;
    }
    // We have an invocation of groupby which is a simple unpacking
    let ExprKind::Name { id: group_name, .. } = &elts[1].node else {
        return;
    };
    let mut finder = GroupNameFinder::new(group_name);
    for stmt in body.iter() {
        finder.visit_stmt(stmt);
    }
    for expr in finder.exprs {
        checker
            .diagnostics
            .push(Diagnostic::new(ReuseOfGroupbyGenerator, Range::from(expr)));
    }
}
