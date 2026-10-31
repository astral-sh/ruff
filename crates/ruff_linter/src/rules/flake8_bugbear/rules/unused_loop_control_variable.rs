use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast as ast;
use ruff_python_ast::helpers;
use ruff_python_ast::helpers::{NameFinder, StoredNameFinder};
use ruff_python_ast::visitor::Visitor;
use ruff_python_semantic::{Binding, BindingKind};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::{Edit, Fix, FixAvailability, Violation};

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
/// ## Options
///
/// - `lint.dummy-variable-rgx`
///
/// ## References
/// - [PEP 8: Naming Conventions](https://peps.python.org/pep-0008/#naming-conventions)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.84", category = Category::Pedantic)]
pub(crate) struct UnusedLoopControlVariable {
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
pub(crate) fn unused_loop_control_variable(checker: &Checker, stmt_for: &ast::StmtFor) {
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

    #[expect(
        clippy::iter_over_hash_type,
        reason = "iteration order does not affect the diagnostics or fixes produced"
    )]
    for (name, expr) in control_names {
        // Ignore names that are already underscore-prefixed.
        if checker.settings().dummy_variable_rgx.is_match(name) {
            continue;
        }

        // Ignore names whose loop binding or a same-scope rebind is used in the loop body. A
        // same-named reference in a nested scope (e.g., a comprehension) may resolve to a
        // different binding, and nested loop variables should not count as uses of this binding.
        let semantic = checker.semantic();
        let scope = semantic.current_scope();
        // A nested loop that reuses the same target is already reported by PLW2901. Avoid
        // reporting B007 for the outer target as well, even though the nested target does not
        // count as a use of the outer binding.
        let redefined_in_nested_loop = scope.get_all(name).any(|binding_id| {
            let binding = semantic.binding(binding_id);
            matches!(&binding.kind, BindingKind::LoopVar)
                && binding.start() > expr.start()
                && stmt_for
                    .body
                    .iter()
                    .any(|stmt| binding.start() >= stmt.start() && binding.end() <= stmt.end())
        });
        if redefined_in_nested_loop {
            continue;
        }
        let binding_id = scope
            .get_all(name)
            .find(|binding_id| semantic.binding(*binding_id).start() == expr.start());
        let used_in_body = binding_id
            .map(|_| {
                scope.get_all(name).any(|binding_id| {
                    let binding = semantic.binding(binding_id);
                    let is_target = binding.start() == expr.start();
                    if !is_target && matches!(&binding.kind, BindingKind::LoopVar) {
                        return false;
                    }

                    // `NameFinder` treated same-scope stores as uses. Preserve that conservative
                    // behavior, while excluding nested loop targets above.
                    if !is_target
                        && matches!(
                            &binding.kind,
                            BindingKind::Assignment
                                | BindingKind::NamedExprAssignment
                                | BindingKind::Annotation
                                | BindingKind::WithItemVar
                                | BindingKind::BoundException
                                | BindingKind::Deletion
                        )
                        && stmt_for.body.iter().any(|stmt| {
                            binding.start() >= stmt.start() && binding.end() <= stmt.end()
                        })
                    {
                        return true;
                    }

                    // Deletion targets are references too. Treat them conservatively as uses to
                    // match `NameFinder`, including conditional deletions that don't create a
                    // separate deletion binding.
                    binding.references().any(|reference_id| {
                        let reference = semantic.reference(reference_id);
                        stmt_for.body.iter().any(|stmt| {
                            reference.start() >= stmt.start() && reference.end() <= stmt.end()
                        })
                    })
                })
            })
            .unwrap_or_else(|| used_names.contains_key(name));
        if used_in_body {
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
            .settings()
            .dummy_variable_rgx
            .is_match(rename.as_str())
            .then_some(rename);

        let mut diagnostic = checker.report_diagnostic(
            UnusedLoopControlVariable {
                name: name.to_string(),
                rename: rename.clone(),
                certainty,
            },
            expr.range(),
        );

        if certainty == Certainty::Certain {
            diagnostic.add_primary_tag(ruff_db::diagnostic::DiagnosticTag::Unnecessary);
        }

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
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum Certainty {
    Certain,
    Uncertain,
}
