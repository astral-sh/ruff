use ruff_text_size::{TextRange, TextSize};

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_trivia::is_python_whitespace;
use ruff_source_file::Locator;

/// ## What it does
/// Checks for whitespace before a shebang directive.
///
/// ## Why is this bad?
/// In Python, a shebang (also known as a hashbang) is the first line of a
/// script, which specifies the interpreter that should be used to run the
/// script.
///
/// The shebang's `#!` prefix must be the first two characters of a file. The
/// presence of whitespace before the shebang will cause the shebang to be
/// ignored, which is likely a mistake.
///
/// ## Example
/// ```python
///  #!/usr/bin/env python3
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
pub struct ShebangLeadingWhitespace;

impl AlwaysFixableViolation for ShebangLeadingWhitespace {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Avoid whitespace before shebang")
    }

    fn fix_title(&self) -> String {
        format!("Remove whitespace before shebang")
    }
}

/// EXE004
pub(crate) fn shebang_leading_whitespace(
    range: TextRange,
    locator: &Locator,
) -> Option<Diagnostic> {
    // If the shebang is at the beginning of the file, abort.
    if range.start() == TextSize::from(0) {
        return None;
    }

    // If the entire prefix _isn't_ whitespace, abort (this is handled by EXE005).
    if !locator
        .up_to(range.start())
        .chars()
        .all(|c| is_python_whitespace(c) || matches!(c, '\r' | '\n'))
    {
        return None;
    }

    let prefix = TextRange::up_to(range.start());
    let mut diagnostic = Diagnostic::new(ShebangLeadingWhitespace, prefix);
    diagnostic.set_fix(Fix::safe_edit(Edit::range_deletion(prefix)));
    Some(diagnostic)
}
