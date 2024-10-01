use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_python_ast::helpers;
use ruff_python_ast::helpers::{NameFinder, StoredNameFinder};
use ruff_python_ast::visitor::Visitor;
use ruff_python_semantic::Binding;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for unused variables in loops (e.g., `for` and `while` statements).
///
/// ## Why is this bad?
/// Defining a variable in a loop statement that is never used can confuse
/// readers.
///
/// If the variable is intended to be unused (e.g., to facilitate
/// destructuring of a tuple or other object), prefix it with an underscore
/// to indicate the intent. Otherwise, remove the variable entirely.
///
/// ## Example
/// ```python
/// for i, j in foo:
///     bar(i)
/// ```
///
/// Use instead:
/// ```python
/// for i, _j in foo:
///     bar(i)
/// ```
///
/// ## References
/// - [PEP 8: Naming Conventions](https://peps.python.org/pep-0008/#naming-conventions)
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
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let UnusedLoopControlVariable {
            name, certainty, ..
        } = self;
        match certainty {
            Certainty::Certain => {
                format!("Loop control variable `{name}` not used within loop body")
            }
            Certainty::Uncertain => {
                format!("Loop control variable `{name}` may not be used within loop body")
            }
        }
    }

    fn fix_title(&self) -> Option<String> {
        let UnusedLoopControlVariable { rename, name, .. } = self;

        rename
            .as_ref()
            .map(|rename| format!("Rename unused `{name}` to `{rename}`"))
    }
}

/// B007
pub(crate) fn unused_loop_control_variable(checker: &mut Checker, stmt_for: &ast::StmtFor) {
    let control_names = {
        let mut finder = StoredNameFinder::default();
        finder.visit_expr(stmt_for.target.as_ref());
        finder.names
    };

    let used_names = {
        let mut finder = NameFinder::default();
        for stmt in &stmt_for.body {
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
        let certainty = if helpers::uses_magic_variable_access(&stmt_for.body, |id| {
            checker.semantic().has_builtin_binding(id)
        }) {
            Certainty::Uncertain
        } else {
            Certainty::Certain
        };

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
            if certainty == Certainty::Certain {
                // Avoid fixing if the variable, or any future bindings to the variable, are
                // used _after_ the loop.
                let scope = checker.semantic().current_scope();
                if scope
                    .get_all(name)
                    .map(|binding_id| checker.semantic().binding(binding_id))
                    .filter(|binding| binding.start() >= expr.start())
                    .all(Binding::is_unused)
                {
                    diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
                        rename,
                        expr.range(),
                    )));
                }
            }
        }
        checker.diagnostics.push(diagnostic);
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum Certainty {
    Certain,
    Uncertain,
}
