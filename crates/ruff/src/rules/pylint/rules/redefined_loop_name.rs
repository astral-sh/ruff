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
            _ => panic!("redefined_loop_name called Statement that is not With, For, or AsyncFor"),
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
