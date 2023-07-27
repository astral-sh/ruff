use ruff_text_size::{TextRange, TextSize};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_trivia::is_python_whitespace;
use ruff_source_file::Locator;

/// ## What it does
/// Checks for a shebang directive that is not at the beginning of the file.
///
/// ## Why is this bad?
/// In Python, a shebang (also known as a hashbang) is the first line of a
/// script, which specifies the interpreter that should be used to run the
/// script.
///
/// The shebang's `#!` prefix must be the first two characters of a file. If
/// the shebang is not at the beginning of the file, it will be ignored, which
/// is likely a mistake.
///
/// ## Example
/// ```python
/// foo = 1
/// #!/usr/bin/env python3
/// ```
///
/// Use instead:
/// ```python
/// #!/usr/bin/env python3
/// foo = 1
/// ```
///
/// ## References
/// - [Python documentation: Executable Python Scripts](https://docs.python.org/3/tutorial/appendix.html#executable-python-scripts)
#[violation]
pub struct ShebangNotFirstLine;

impl Violation for ShebangNotFirstLine {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Shebang should be at the beginning of the file")
    }
}

/// EXE005
pub(crate) fn shebang_not_first_line(range: TextRange, locator: &Locator) -> Option<Diagnostic> {
    // If the shebang is at the beginning of the file, abort.
    if range.start() == TextSize::from(0) {
        return None;
    }

    // If the entire prefix is whitespace, abort (this is handled by EXE004).
    if locator
        .up_to(range.start())
        .chars()
        .all(|c| is_python_whitespace(c) || matches!(c, '\r' | '\n'))
    {
        return None;
    }

    Some(Diagnostic::new(ShebangNotFirstLine, range))
}
