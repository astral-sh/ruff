//! Checks for unused loop variables.
//!
//! ## Why is this bad?
//!
//! Unused variables may signal a mistake or unfinished code.
//!
//! ## Example
//!
//! ```python
//! for x in range(10):
//!     method()
//! ```
//!
//! Prefix the variable with an underscore:
//!
//! ```python
//! for _x in range(10):
//!     method()
//! ```

use rustc_hash::FxHashMap;
use rustpython_ast::{Expr, ExprKind, Stmt};
use serde::{Deserialize, Serialize};

use ruff_macros::derive_message_formats;

use crate::ast::types::{BindingKind, Range, RefEquality};
use crate::ast::visitor::Visitor;
use crate::ast::{helpers, visitor};
use crate::checkers::ast::Checker;
use crate::define_violation;
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::violation::{AutofixKind, Availability, Violation};

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Certainty {
    Certain,
    Uncertain,
}

define_violation!(
    pub struct UnusedLoopControlVariable {
        /// The name of the loop control variable.
        pub name: String,
        /// Whether the variable is certain to be unused, or merely suspect.
        /// A variable _may_ be used, but undetectably so, if the loop incorporates
        /// by magic control flow (e.g., `locals()`).
        pub certainty: Certainty,
    }
);
impl Violation for UnusedLoopControlVariable {
    const AUTOFIX: Option<AutofixKind> = Some(AutofixKind::new(Availability::Always));

    #[derive_message_formats]
    fn message(&self) -> String {
        let UnusedLoopControlVariable { name, certainty } = self;
        if matches!(certainty, Certainty::Certain) {
            format!("Loop control variable `{name}` not used within loop body")
        } else {
            format!("Loop control variable `{name}` may not be used within loop body")
        }
    }

    fn autofix_title_formatter(&self) -> Option<fn(&Self) -> String> {
        let UnusedLoopControlVariable { certainty, .. } = self;
        if matches!(certainty, Certainty::Certain) {
            Some(|UnusedLoopControlVariable { name, .. }| {
                format!("Rename unused `{name}` to `_{name}`")
            })
        } else {
            None
        }
    }
}

/// Identify all `ExprKind::Name` nodes in an AST.
struct NameFinder<'a> {
    /// A map from identifier to defining expression.
    names: FxHashMap<&'a str, &'a Expr>,
}

impl NameFinder<'_> {
    fn new() -> Self {
        NameFinder {
            names: FxHashMap::default(),
        }
    }
}

impl<'a, 'b> Visitor<'b> for NameFinder<'a>
where
    'b: 'a,
{
    fn visit_expr(&mut self, expr: &'a Expr) {
        if let ExprKind::Name { id, .. } = &expr.node {
            self.names.insert(id, expr);
        }
        visitor::walk_expr(self, expr);
    }
}

/// B007
pub fn unused_loop_control_variable(
    checker: &mut Checker,
    stmt: &Stmt,
    target: &Expr,
    body: &[Stmt],
) {
    let control_names = {
        let mut finder = NameFinder::new();
        finder.visit_expr(target);
        finder.names
    };

    let used_names = {
        let mut finder = NameFinder::new();
        for stmt in body {
            finder.visit_stmt(stmt);
        }
        finder.names
    };

    for (name, expr) in control_names {
        // Ignore names that are already underscore-prefixed.
        if checker.settings.dummy_variable_rgx.is_match(name) {
            continue;
        }

        // Ignore any names that are actually used in the loop body.
        if used_names.contains_key(name) {
            continue;
        }

        let certainty = if helpers::uses_magic_variable_access(checker, body) {
            Certainty::Uncertain
        } else {
            Certainty::Certain
        };
        let mut diagnostic = Diagnostic::new(
            UnusedLoopControlVariable {
                name: name.to_string(),
                certainty,
            },
            Range::from_located(expr),
        );
        if matches!(certainty, Certainty::Certain) && checker.patch(diagnostic.kind.rule()) {
            // Guard against cases in which the loop variable is used later on in the scope.
            // TODO(charlie): This avoids fixing cases in which the loop variable is overridden (but
            // unused) later on in the scope.
            if let Some(binding) = checker.find_binding(name) {
                if matches!(binding.kind, BindingKind::LoopVar) {
                    if !binding.used() {
                        if let Some(source) = &binding.source {
                            if source == &RefEquality(stmt) {
                                // Prefix the variable name with an underscore.
                                diagnostic.amend(Fix::replacement(
                                    format!("_{name}"),
                                    expr.location,
                                    expr.end_location.unwrap(),
                                ));
                            }
                        }
                    }
                }
            }
        }
        checker.diagnostics.push(diagnostic);
    }
}
