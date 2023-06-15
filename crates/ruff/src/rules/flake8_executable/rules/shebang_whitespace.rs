use ruff_text_size::{TextRange, TextSize};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};

use crate::rules::flake8_executable::helpers::ShebangDirective;

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

impl AlwaysAutofixableViolation for ShebangLeadingWhitespace {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Avoid whitespace before shebang")
    }

    fn autofix_title(&self) -> String {
        format!("Remove whitespace before shebang")
    }
}

/// EXE004
pub(crate) fn shebang_whitespace(
    range: TextRange,
    shebang: &ShebangDirective,
    autofix: bool,
) -> Option<Diagnostic> {
    if let ShebangDirective::Match(n_spaces, start, ..) = shebang {
        if *n_spaces > TextSize::from(0) && *start == n_spaces + TextSize::from(2) {
            let mut diagnostic = Diagnostic::new(
                ShebangLeadingWhitespace,
                TextRange::at(range.start(), *n_spaces),
            );
            if autofix {
                diagnostic.set_fix(Fix::automatic(Edit::range_deletion(TextRange::at(
                    range.start(),
                    *n_spaces,
                ))));
            }
            Some(diagnostic)
        } else {
            None
        }
    } else {
        None
    }
}
