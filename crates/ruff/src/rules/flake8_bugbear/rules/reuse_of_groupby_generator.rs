use rustpython_parser::ast::{Expr, ExprKind, Stmt, StmtKind};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;
use ruff_python_ast::visitor::{self, Visitor};

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
        format!("Using the generator returned from `itertools.groupby()` more than once will do nothing on the second usage")
    }
}

/// A [`Visitor`] that collects all the usage of `group_name` in the body of
/// a `for` loop.
struct GroupNameFinder<'a> {
    /// Variable name for the group.
    group_name: &'a str,
    /// Number of times the `group_name` variable was seen during the visit.
    usage_count: u8,
    /// A flag indicating that the visitor is inside a nested `for` or `while`
    /// loop or inside a `dict`, `list` or `set` comprehension.
    nested: bool,
    /// A flag indicating that the `group_name` variable has been overridden
    /// during the visit.
    overridden: bool,
    /// A list of reused expressions.
    exprs: Vec<&'a Expr>,
}

impl<'a> GroupNameFinder<'a> {
    fn new(group_name: &'a str) -> Self {
        GroupNameFinder {
            group_name,
            usage_count: 0,
            nested: false,
            overridden: false,
            exprs: Vec::new(),
        }
    }

    fn name_matches(&self, expr: &Expr) -> bool {
        if let ExprKind::Name { id, .. } = &expr.node {
            id == self.group_name
        } else {
            false
        }
    }
}

impl<'a, 'b> Visitor<'b> for GroupNameFinder<'a>
where
    'b: 'a,
{
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        if self.overridden {
            return;
        }
        match &stmt.node {
            StmtKind::For {
                target, iter, body, ..
            } => {
                if self.name_matches(target) {
                    self.overridden = true;
                } else {
                    if self.name_matches(iter) {
                        self.usage_count += 1;
                        // This could happen when the group is being looped
                        // over multiple times:
                        //      for item in group:
                        //          ...
                        //
                        //      # Group is being reused here
                        //      for item in group:
                        //          ...
                        if self.usage_count > 1 {
                            self.exprs.push(iter);
                        }
                    }
                    self.nested = true;
                    visitor::walk_body(self, body);
                    self.nested = false;
                }
            }
            StmtKind::While { body, .. } => {
                self.nested = true;
                visitor::walk_body(self, body);
                self.nested = false;
            }
            StmtKind::Assign { targets, .. } => {
                if targets.iter().any(|target| self.name_matches(target)) {
                    self.overridden = true;
                }
            }
            StmtKind::AnnAssign { target, .. } => {
                if self.name_matches(target) {
                    self.overridden = true;
                }
            }
            _ => visitor::walk_stmt(self, stmt),
        }
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        if let ExprKind::NamedExpr { target, .. } = &expr.node {
            if self.name_matches(target) {
                self.overridden = true;
            }
        }
        if self.overridden {
            return;
        }
        if matches!(
            &expr.node,
            ExprKind::ListComp { .. } | ExprKind::DictComp { .. } | ExprKind::SetComp { .. }
        ) {
            self.nested = true;
            visitor::walk_expr(self, expr);
            self.nested = false;
        } else if self.name_matches(expr) {
            self.usage_count += 1;
            // For nested loops, the variable usage could be once but the
            // loop makes it being used multiple times.
            if self.nested || self.usage_count > 1 {
                self.exprs.push(expr);
            }
        } else {
            visitor::walk_expr(self, expr);
        }
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
