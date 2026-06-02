use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_semantic::{BindingKind, Scope};
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `global` and `nonlocal` declarations placed inside a nested block
/// (such as an `if`, `for`, `while`, `with`, `try`, or `match` body) rather than
/// at the top of the function.
///
/// ## Why is this bad?
/// A `global` or `nonlocal` statement applies to the entire enclosing function,
/// regardless of where it appears. Nesting it inside a block is misleading: it
/// looks as though the declaration is scoped to (or conditional on) that block,
/// when in fact it affects every assignment to the name throughout the function.
///
/// The effect applies even when the block never executes. For example, a `global`
/// inside a `for` loop still takes effect when the loop iterates zero times:
///
/// ```pycon
/// >>> x = 1
/// >>> def f():
/// ...     for _ in range(0):
/// ...         global x
/// ...     x = 2
/// >>> f()
/// >>> x
/// 2
/// ```
///
/// Placing the declaration at the top of the function makes its function-wide
/// scope obvious.
///
/// ## Example
/// ```python
/// counter = 0
///
///
/// def update(flag):
///     if flag:
///         global counter
///         counter = 1
///     else:
///         counter = 2  # also rebinds the global `counter`
/// ```
///
/// Use instead:
///
/// ```python
/// counter = 0
///
///
/// def update(flag):
///     global counter
///     if flag:
///         counter = 1
///     else:
///         counter = 2
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "NEXT_RUFF_VERSION")]
pub(crate) struct NestedGlobalOrNonlocal {
    name: String,
    keyword: &'static str,
}

impl Violation for NestedGlobalOrNonlocal {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NestedGlobalOrNonlocal { name, keyword } = self;
        format!(
            "`{name}` is declared `{keyword}` inside a nested block, but the declaration applies to the entire function"
        )
    }
}

/// RUF076
pub(crate) fn nested_global_or_nonlocal(checker: &Checker, scope: &Scope) {
    let semantic = checker.semantic();

    // `all_bindings` includes shadowed bindings, so the `global`/`nonlocal`
    // declaration is still found when a later assignment shadows it.
    for (name, binding_id) in scope.all_bindings() {
        let binding = &semantic.bindings[binding_id];
        let keyword = match binding.kind {
            BindingKind::Global(_) => "global",
            BindingKind::Nonlocal(_, _) => "nonlocal",
            _ => continue,
        };
        let Some(source) = binding.source else {
            continue;
        };

        // A `global`/`nonlocal` at the top of the function body has the function
        // definition as its parent statement. Any other parent means the
        // declaration is nested inside a block (`if`, `for`, `while`, `with`,
        // `try`, or `match`), where its function-wide effect is easy to miss.
        if semantic
            .parent_statement(source)
            .is_some_and(|parent| !parent.is_function_def_stmt())
        {
            checker.report_diagnostic(
                NestedGlobalOrNonlocal {
                    name: name.to_string(),
                    keyword,
                },
                binding.range(),
            );
        }
    }
}
