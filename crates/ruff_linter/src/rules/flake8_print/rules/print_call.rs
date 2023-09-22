use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::is_const_none;
use ruff_python_ast::{self as ast};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

/// ## What it does
/// Checks for `print` statements.
///
/// ## Why is this bad?
/// `print` statements are useful in some situations (e.g., debugging), but
/// should typically be omitted from production code. `print` statements can
/// lead to the accidental inclusion of sensitive information in logs, and are
/// not configurable by clients, unlike `logging` statements.
///
/// ## Example
/// ```python
/// def add_numbers(a, b):
///     print(f"The sum of {a} and {b} is {a + b}")
///     return a + b
/// ```
///
/// Use instead:
/// ```python
/// def add_numbers(a, b):
///     return a + b
/// ```
#[violation]
pub struct Print;

impl Violation for Print {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`print` found")
    }
}

/// ## What it does
/// Checks for `pprint` statements.
///
/// ## Why is this bad?
/// Like `print` statements, `pprint` statements are useful in some situations
/// (e.g., debugging), but should typically be omitted from production code.
/// `pprint` statements can lead to the accidental inclusion of sensitive
/// information in logs, and are not configurable by clients, unlike `logging`
/// statements.
///
/// ## Example
/// ```python
/// import pprint
///
///
/// def merge_dicts(dict_a, dict_b):
///     dict_c = {**dict_a, **dict_b}
///     pprint.pprint(dict_c)
///     return dict_c
/// ```
///
/// Use instead:
/// ```python
/// def merge_dicts(dict_a, dict_b):
///     dict_c = {**dict_a, **dict_b}
///     return dict_c
/// ```
#[violation]
pub struct PPrint;

impl Violation for PPrint {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`pprint` found")
    }
}

/// T201, T203
pub(crate) fn print_call(checker: &mut Checker, call: &ast::ExprCall) {
    let diagnostic = {
        let call_path = checker.semantic().resolve_call_path(&call.func);
        if call_path
            .as_ref()
            .is_some_and(|call_path| matches!(call_path.as_slice(), ["", "print"]))
        {
            // If the print call has a `file=` argument (that isn't `None`, `"sys.stdout"`,
            // or `"sys.stderr"`), don't trigger T201.
            if let Some(keyword) = call.arguments.find_keyword("file") {
                if !is_const_none(&keyword.value) {
                    if checker.semantic().resolve_call_path(&keyword.value).map_or(
                        true,
                        |call_path| {
                            call_path.as_slice() != ["sys", "stdout"]
                                && call_path.as_slice() != ["sys", "stderr"]
                        },
                    ) {
                        return;
                    }
                }
            }
            Diagnostic::new(Print, call.func.range())
        } else if call_path
            .as_ref()
            .is_some_and(|call_path| matches!(call_path.as_slice(), ["pprint", "pprint"]))
        {
            Diagnostic::new(PPrint, call.func.range())
        } else {
            return;
        }
    };

    if !checker.enabled(diagnostic.kind.rule()) {
        return;
    }

    checker.diagnostics.push(diagnostic);
}
