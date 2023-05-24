use ruff_python_ast::source_code::Locator;
use ruff_text_size::{TextRange, TextSize};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::newlines::Line;

/// ## What it does
/// Checks for too many (>=3) blank lines.
///
/// ## Why is this bad?
/// PEP 8 recommends the using blank lines as following:
/// - Two blank lines are expected between functions and classes
/// - One blank line is expected between methods of a class.
///
/// ## Example
/// ```python
/// def func1():
///     pass
///
///
///
/// def func2():
///     pass
/// ```
///
/// Use instead:
/// ```python
/// def func1():
///     pass
///
///
/// def func2():
///     pass
/// ```
///
/// ## References
/// - [PEP 8](https://peps.python.org/pep-0008/#blank-lines)
#[violation]
pub struct TooManyBlankLines(pub usize);

impl AlwaysAutofixableViolation for TooManyBlankLines {
    #[derive_message_formats]
    fn message(&self) -> String {
        let TooManyBlankLines(nb_blank_lines) = self;
        format!("Too many blank lines ({nb_blank_lines})")
    }

    fn autofix_title(&self) -> String {
        "Remove extraneous blank line(s)".to_string()
    }
}

/// E303
pub(crate) fn too_many_blank_lines(line: &Line, locator: &Locator) -> Option<Diagnostic> {
    // Only check for too many blank lines starting from the first blank line of a (potential) series
    // of blank lines (to avoid duplicate errors).
    // Also ignore blank lines at the beginning of the file.
    if line.start().to_u32() > 0
        && line.trim().is_empty()
        && !locator
            .line(TextSize::new(line.start().to_u32() - 1))
            .trim()
            .is_empty()
    {
        let mut nb_blank_lines = 0;
        let mut previous_line_end = line.end();
        loop {
            nb_blank_lines += 1;
            previous_line_end = locator.full_line_end(previous_line_end);
            let previous_line = locator.line(previous_line_end);

            if !previous_line.trim().is_empty() || previous_line_end >= locator.text_len() {
                break;
            }
        }

        // Generate a diagnostic if there are too many blank lines not at the end of the file.
        if nb_blank_lines > 2 && previous_line_end < locator.text_len() {
            let last_blank_line = TextSize::new(locator.line_start(previous_line_end).to_u32() - 1);
            let range = locator.full_lines_range(TextRange::new(line.start(), last_blank_line));
            let mut diagnostic = Diagnostic::new(TooManyBlankLines(nb_blank_lines), range);
            diagnostic.set_fix(Fix::suggested(Edit::range_replacement(
                "\n\n".to_string(),
                range,
            )));
            return Some(diagnostic);
        }
    }

    None
}
