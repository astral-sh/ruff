use ruff_text_size::TextRange;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::comments::shebang::ShebangDirective;

/// ## What it does
/// Checks for a shebang directive in `.py` files that does not contain `python`.
///
/// ## Why is this bad?
/// In Python, a shebang (also known as a hashbang) is the first line of a
/// script, which specifies the interpreter that should be used to run the
/// script.
///
/// For Python scripts, the shebang must contain `python` to indicate that the
/// script should be executed as a Python script. If the shebang does not
/// contain `python`, then the file will be executed with the default
/// interpreter, which is likely a mistake.
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
#[violation]
pub struct ShebangMissingPython;

impl Violation for ShebangMissingPython {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Shebang should contain `python`")
    }
}

/// EXE003
pub(crate) fn shebang_missing_python(
    range: TextRange,
    shebang: &ShebangDirective,
) -> Option<Diagnostic> {
    if shebang.contains("python") || shebang.contains("pytest") {
        return None;
    }

    Some(Diagnostic::new(ShebangMissingPython, range))
}
