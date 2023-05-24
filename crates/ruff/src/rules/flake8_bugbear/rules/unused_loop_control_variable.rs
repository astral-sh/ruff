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
use rustpython_parser::ast::{self, Expr, Ranged, Stmt};
use serde::{Deserialize, Serialize};

use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::{helpers, visitor};

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize, result_like::BoolLike)]
enum Certainty {
    Certain,
    Uncertain,
}

#[violation]
pub struct UnusedLoopControlVariable {
    /// The name of the loop control variable.
    name: String,
    /// The name to which the variable should be renamed, if it can be
    /// safely renamed.
    rename: Option<String>,
    /// Whether the variable is certain to be unused in the loop body, or
    /// merely suspect. A variable _may_ be used, but undetectably
    /// so, if the loop incorporates by magic control flow (e.g.,
    /// `locals()`).
    certainty: Certainty,
}

impl Violation for UnusedLoopControlVariable {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let UnusedLoopControlVariable {
            name, certainty, ..
        } = self;
        if certainty.to_bool() {
            format!("Loop control variable `{name}` not used within loop body")
        } else {
            format!("Loop control variable `{name}` may not be used within loop body")
        }
    }

    fn autofix_title(&self) -> Option<String> {
        let UnusedLoopControlVariable { rename, name, .. } = self;

        rename
            .as_ref()
            .map(|rename| format!("Rename unused `{name}` to `{rename}`"))
    }
}

/// Identify all `Expr::Name` nodes in an AST.
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
        if let Expr::Name(ast::ExprName { id, .. }) = expr {
            self.names.insert(id, expr);
        }
        visitor::walk_expr(self, expr);
    }
}

/// B007
pub(crate) fn unused_loop_control_variable(checker: &mut Checker, target: &Expr, body: &[Stmt]) {
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

        // Avoid fixing any variables that _may_ be used, but undetectably so.
        let certainty = Certainty::from(!helpers::uses_magic_variable_access(body, |id| {
            checker.semantic_model().is_builtin(id)
        }));

        // Attempt to rename the variable by prepending an underscore, but avoid
        // applying the fix if doing so wouldn't actually cause us to ignore the
        // violation in the next pass.
        let rename = format!("_{name}");
        let rename = checker
            .settings
            .dummy_variable_rgx
            .is_match(rename.as_str())
            .then_some(rename);

        let mut diagnostic = Diagnostic::new(
            UnusedLoopControlVariable {
                name: name.to_string(),
                rename: rename.clone(),
                certainty,
            },
            expr.range(),
        );
        if let Some(rename) = rename {
            if certainty.into() && checker.patch(diagnostic.kind.rule()) {
                // Avoid fixing if the variable, or any future bindings to the variable, are
                // used _after_ the loop.
                let scope = checker.semantic_model().scope();
                if scope
                    .bindings_for_name(name)
                    .map(|binding_id| &checker.semantic_model().bindings[*binding_id])
                    .all(|binding| !binding.is_used())
                {
                    #[allow(deprecated)]
                    diagnostic.set_fix(Fix::suggested(Edit::range_replacement(
                        rename,
                        expr.range(),
                    )));
                }
            }
        }
        checker.diagnostics.push(diagnostic);
    }
}
