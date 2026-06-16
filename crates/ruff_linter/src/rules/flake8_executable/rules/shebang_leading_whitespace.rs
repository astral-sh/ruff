use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_trivia::is_python_whitespace;
use ruff_text_size::{TextRange, TextSize};

use crate::Locator;
use crate::checkers::ast::LintContext;
use crate::{AlwaysFixableViolation, Edit, Fix};

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
/// ## Fix safety
/// This rule's fix is marked as unsafe when the whitespace before the shebang
/// contains a newline. Deleting the newline shifts the following lines up,
/// which can move an encoding declaration onto the second line, where Python
/// honors it as a magic encoding comment (PEP 263) and may change how the file
/// is decoded. When the whitespace contains no newline, the shebang is already
/// on the first line and the fix is safe.
///
/// ## References
/// - [Python documentation: Executable Python Scripts](https://docs.python.org/3/tutorial/appendix.html#executable-python-scripts)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.229")]
pub(crate) struct ShebangLeadingWhitespace;

impl AlwaysFixableViolation for ShebangLeadingWhitespace {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Avoid whitespace before shebang".to_string()
    }

    fn fix_title(&self) -> String {
        "Remove whitespace before shebang".to_string()
    }
}

/// EXE004
pub(crate) fn shebang_leading_whitespace(
    context: &LintContext,
    range: TextRange,
    locator: &Locator,
) {
    // If the shebang is at the beginning of the file, abort.
    if range.start() == TextSize::from(0) {
        return;
    }

    // If the entire prefix _isn't_ whitespace, abort (this is handled by EXE005).
    if !locator
        .up_to(range.start())
        .chars()
        .all(|c| is_python_whitespace(c) || matches!(c, '\r' | '\n'))
    {
        return;
    }

    let prefix = TextRange::up_to(range.start());
    if let Some(mut diagnostic) =
        context.report_diagnostic_if_enabled(ShebangLeadingWhitespace, prefix)
    {
        // The fix is only unsafe when the leading whitespace contains a newline:
        // deleting it shifts the following lines up, which can move an encoding
        // declaration onto the second line, where Python honors it as a magic
        // encoding comment (PEP 263) and may change how the file is decoded.
        // Without a newline the shebang is already on the first line and the fix
        // moves no other lines, so it is safe.
        let fix = if locator
            .up_to(range.start())
            .chars()
            .any(|c| matches!(c, '\r' | '\n'))
        {
            Fix::unsafe_edit(Edit::range_deletion(prefix))
        } else {
            Fix::safe_edit(Edit::range_deletion(prefix))
        };
        diagnostic.set_fix(fix);
    }
}
