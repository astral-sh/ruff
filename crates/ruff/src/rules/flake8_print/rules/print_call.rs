use rustpython_parser::ast::{Expr, Keyword};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::is_const_none;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

/// ## What it does
/// Checks for `print` statements.
///
/// ## Why is this bad?
/// `print` statements are useful in some situations (e.g. debugging), but they have a few downsides:
/// - They can make it harder to maintain code as it can be challenging to keep track of numerous
/// print statements as a codebase grows larger.
/// - `print` statements can be slow, especially if they are printing large amounts of information.
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

#[violation]
pub struct PPrint;

impl Violation for PPrint {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`pprint` found")
    }
}

/// T201, T203
pub fn print_call(checker: &mut Checker, func: &Expr, keywords: &[Keyword]) {
    let diagnostic = {
        let call_path = checker.ctx.resolve_call_path(func);
        if call_path
            .as_ref()
            .map_or(false, |call_path| *call_path.as_slice() == ["", "print"])
        {
            // If the print call has a `file=` argument (that isn't `None`, `"sys.stdout"`,
            // or `"sys.stderr"`), don't trigger T201.
            if let Some(keyword) = keywords
                .iter()
                .find(|keyword| keyword.node.arg.as_ref().map_or(false, |arg| arg == "file"))
            {
                if !is_const_none(&keyword.node.value) {
                    if checker.ctx.resolve_call_path(&keyword.node.value).map_or(
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
            Diagnostic::new(Print, func.range())
        } else if call_path.as_ref().map_or(false, |call_path| {
            *call_path.as_slice() == ["pprint", "pprint"]
        }) {
            Diagnostic::new(PPrint, func.range())
        } else {
            return;
        }
    };

    if !checker.settings.rules.enabled(diagnostic.kind.rule()) {
        return;
    }

    checker.diagnostics.push(diagnostic);
}
