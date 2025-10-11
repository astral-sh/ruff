use ruff_text_size::TextRange;

use ruff_macros::{ViolationMetadata, derive_message_formats};

use crate::Violation;
use crate::checkers::ast::LintContext;
use crate::comments::shebang::ShebangDirective;

/// ## What it does
/// Checks for a shebang directive in `.py` files that does not contain `python`,
/// `pytest`, or `uv run`.
///
/// ## Why is this bad?
/// In Python, a shebang (also known as a hashbang) is the first line of a
/// script, which specifies the command that should be used to run the
/// script.
///
/// For Python scripts, if the shebang does not include a command that explicitly
/// or implicitly specifies an interpreter, then the file will be executed with
/// the default interpreter, which is likely a mistake.
///
/// ## Example
/// ```python
/// #!/usr/bin/env bash
/// ```
///
/// Use instead:
/// ```python
/// #!/usr/bin/env python3
/// ```
///
/// ## References
/// - [Python documentation: Executable Python Scripts](https://docs.python.org/3/tutorial/appendix.html#executable-python-scripts)
#[derive(ViolationMetadata)]
pub(crate) struct ShebangMissingPython;

impl Violation for ShebangMissingPython {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Shebang should contain `python`, `pytest`, or `uv run`".to_string()
    }
}

/// EXE003
pub(crate) fn shebang_missing_python(
    range: TextRange,
    shebang: &ShebangDirective,
    context: &LintContext,
) {
    if shebang.contains("python")
        || shebang.contains("pytest")
        || shebang.contains("uv run")
        || shebang.contains("uvx")
        || shebang.contains("uv tool run")
    {
        return;
    }

    context.report_diagnostic_if_enabled(ShebangMissingPython, range);
}
