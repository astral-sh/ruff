use rustc_hash::FxHashMap;
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast as ast;
use ruff_python_ast::helpers;
use ruff_python_ast::helpers::{NameFinder, StoredNameFinder};
use ruff_python_ast::visitor::Visitor;
use ruff_python_semantic::Binding;
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
    /// If the loop iterates over `dict.items()` and only one half of the
    /// unpacked pair is used, this is `"keys"` or `"values"`; the fix then
    /// changes the loop to iterate the corresponding dict view instead of
    /// merely renaming the unused half (#29939).
    dict_view: Option<String>,
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
        let UnusedLoopControlVariable {
            rename,
            name,
            dict_view,
            ..
        } = self;

        if let Some(view) = &dict_view {
            return Some(format!("Use `.{view}()` and iterate the used half"));
        }

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
            .settings()
            .dummy_variable_rgx
            .is_match(rename.as_str())
            .then_some(rename);

        let dict_items_view = unused_dict_items_view(
            checker,
            stmt_for,
            &used_names,
        )
        .filter(|suggestion| suggestion.unused_name == name);

        let mut diagnostic = checker.report_diagnostic(
            UnusedLoopControlVariable {
                name: name.to_string(),
                rename: rename.clone(),
                dict_view: dict_items_view
                    .as_ref()
                    .map(|suggestion| suggestion.view.to_string()),
                certainty,
            },
            expr.range(),
        );

        if certainty == Certainty::Certain {
            diagnostic.add_primary_tag(ruff_db::diagnostic::DiagnosticTag::Unnecessary);
        }

        if let Some(dict_items_view) = dict_items_view
            && certainty == Certainty::Certain
        {
            // Avoid fixing if the unused variable, or any future bindings
            // to it, are used _after_ the loop.
            let scope = checker.semantic().current_scope();
            if scope
                .get_all(name)
                .map(|binding_id| checker.semantic().binding(binding_id))
                .filter(|binding| binding.start() >= expr.start())
                .all(Binding::is_unused)
            {
                diagnostic.set_fix(Fix::unsafe_edits(
                    Edit::range_replacement(
                        dict_items_view.view.to_string(),
                        dict_items_view.attr_range,
                    ),
                    [Edit::range_replacement(
                        dict_items_view.used_name.to_string(),
                        stmt_for.target.range(),
                    )],
                ));
            }
        } else if let Some(rename) = rename {
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

struct DictItemsView<'a> {
    view: &'static str,
    used_name: &'a str,
    unused_name: &'a str,
    attr_range: ruff_text_size::TextRange,
}

fn unused_dict_items_view<'a>(
    checker: &Checker,
    stmt_for: &'a ast::StmtFor,
    used_names: &FxHashMap<&'a str, &'a ast::ExprName>,
) -> Option<DictItemsView<'a>> {
    use ruff_python_semantic::analyze::typing::is_dict;

    let call = stmt_for.iter.as_call_expr()?;
    if !call.arguments.is_empty() {
        return None;
    }
    let Some(attr_expr) = call.func.as_attribute_expr() else {
        return None;
    };
    if attr_expr.attr.id.to_string() != "items" {
        return None;
    }
    let Some(name_expr) = attr_expr.value.as_name_expr() else {
        return None;
    };
    let id = name_expr.id.as_str();
    let Some(binding_id) = checker.semantic().lookup_symbol(id).binding_id() else {
        return None;
    };
    if !is_dict(checker.semantic().binding(binding_id), checker.semantic()) {
        return None;
    }
    let ast::Expr::Tuple(ast::ExprTuple { elts, .. }) = stmt_for.target.as_ref() else {
        return None;
    };
    let [key, value] = elts.as_slice() else {
        return None;
    };
    let key = key.as_name_expr()?.id.as_str();
    let value = value.as_name_expr()?.id.as_str();
    if checker.settings().dummy_variable_rgx.is_match(key)
        || checker.settings().dummy_variable_rgx.is_match(value)
    {
        return None;
    }

    if !used_names.contains_key(key) && used_names.contains_key(value) {
        Some(DictItemsView {
            view: "values",
            used_name: value,
            unused_name: key,
            attr_range: attr_expr.attr.range(),
        })
    } else if !used_names.contains_key(value) && used_names.contains_key(key) {
        Some(DictItemsView {
            view: "keys",
            used_name: key,
            unused_name: value,
            attr_range: attr_expr.attr.range(),
        })
    } else {
        None
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum Certainty {
    Certain,
    Uncertain,
}
