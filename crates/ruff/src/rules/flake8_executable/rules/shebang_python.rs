use ruff_text_size::{TextLen, TextRange, TextSize};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::rules::flake8_executable::helpers::ShebangDirective;

/// ## What it does
/// Checks for a shebang directive in `.py` files that does not contain `python`.
///
/// ## Why is this bad?
/// The shebang directive must contain `python` to indicate that the file is a
/// Python script. If the shebang does not contain `python`, then the file is
/// unlikely to be executed as a Python script. This is either a mistake or
/// misleading.
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
pub(crate) fn shebang_python(range: TextRange, shebang: &ShebangDirective) -> Option<Diagnostic> {
    if let ShebangDirective::Match(_, start, content) = shebang {
        if content.contains("python") || content.contains("pytest") {
            None
        } else {
            let diagnostic = Diagnostic::new(
                ShebangMissingPython,
                TextRange::at(range.start() + start, content.text_len())
                    .sub_start(TextSize::from(2)),
            );

            Some(diagnostic)
        }
    } else {
        None
    }
}
