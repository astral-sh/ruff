use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_source_file::{Locator, UniversalNewlines};
use ruff_text_size::TextRange;

use crate::settings::LinterSettings;

/// ## What it does
/// Checks for modules with too many lines.
///
/// By default, this rule allows up to 2000 lines per module. This can be configured
/// using the [`lint.pylint.max-module-lines`] option.
///
/// ## Why is this bad?
/// When a module has too many lines it can make it difficult to read and understand.
/// There might be performance issue while editing the file because the IDE must parse more code.
/// You need more expertise to navigate the file properly (go to a particular line when debugging,
/// or searching for a specific code construct, instead of navigating by clicking and scrolling).
///
/// ## Example
/// ```python
/// def is_palindrome(string):  # [too-many-lines]
///     left_pos = 0
///     right_pos = len(string) - 1
///     while right_pos >= left_pos:
///         if not string[left_pos] == string[right_pos]:
///             return False
///         left_pos += 1
///         right_pos -= 1
///     return True
///
///
/// def main():
///     print(is_palindrome("aza"))
///     print(is_palindrome("racecar"))
///     print(is_palindrome("trigger"))
///     print(is_palindrome("ogre"))
/// ```
///
/// Use instead:
///
/// `__init__.py`
/// ```python
/// __all__ = ["is_palindrome", "main"]
///
/// from is_palindrome import is_palindrome
/// from main import main
/// ```
///
/// `is_palindrome.py`
/// ```python
/// def is_palindrome(string):
///     return string == string[::-1]
/// ```
///
/// `main.py`
/// ```python
/// from is_palindrome import is_palindrome
///
///
/// def main():
///     for string in ["aza", "racecar", "trigger", "ogre"]:
///         print(is_palindrome(string))
/// ```
///
/// ## Options
/// - `lint.pylint.max-module-lines`
#[violation]
pub struct TooManyLines {
    number_of_lines: usize,
    max_module_lines: usize,
}

impl Violation for TooManyLines {
    #[derive_message_formats]
    fn message(&self) -> String {
        let TooManyLines {
            number_of_lines,
            max_module_lines,
        } = self;
        format!("Too many lines in module ({number_of_lines}>{max_module_lines})")
    }
}

/// PLC0302
pub(crate) fn too_many_lines(locator: &Locator, settings: &LinterSettings) -> Option<Diagnostic> {
    let lines = locator.contents().universal_newlines();
    let number_of_lines = lines.count() + 1;

    if number_of_lines > settings.pylint.max_module_lines {
        let diagnostic = Diagnostic::new(
            TooManyLines {
                number_of_lines,
                max_module_lines: settings.pylint.max_module_lines,
            },
            TextRange::default(),
        );
        return Some(diagnostic);
    }

    None
}
