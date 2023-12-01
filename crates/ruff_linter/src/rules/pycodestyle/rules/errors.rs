use ruff_python_parser::ParseError;
use ruff_text_size::{TextLen, TextRange, TextSize};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_source_file::Locator;

use crate::logging::DisplayParseErrorType;

/// ## What it does
/// This is not a regular diagnostic; instead, it's raised when a file cannot be read
/// from disk.
///
/// ## Why is this bad?
/// An `IOError` indicates an error in the development setup. For example, the user may
/// not have permissions to read a given file, or the filesystem may contain a broken
/// symlink.
///
/// ## Example
/// On Linux or macOS:
/// ```shell
/// $ echo 'print("hello world!")' > a.py
/// $ chmod 000 a.py
/// $ ruff a.py
/// a.py:1:1: E902 Permission denied (os error 13)
/// Found 1 error.
/// ```
///
/// ## References
/// - [UNIX Permissions introduction](https://mason.gmu.edu/~montecin/UNIXpermiss.htm)
/// - [Command Line Basics: Symbolic Links](https://www.digitalocean.com/community/tutorials/workflow-symbolic-links)
#[violation]
pub struct IOError {
    pub message: String,
}

/// E902
impl Violation for IOError {
    #[derive_message_formats]
    fn message(&self) -> String {
        let IOError { message } = self;
        format!("{message}")
    }
}

/// ## What it does
/// Checks for code that contains syntax errors.
///
/// ## Why is this bad?
/// Code with syntax errors cannot be executed. Such errors are likely a
/// mistake.
///
/// ## Example
/// ```python
/// x =
/// ```
///
/// Use instead:
/// ```python
/// x = 1
/// ```
///
/// ## References
/// - [Python documentation: Syntax Errors](https://docs.python.org/3/tutorial/errors.html#syntax-errors)
#[violation]
pub struct SyntaxError {
    pub message: String,
}

impl Violation for SyntaxError {
    #[derive_message_formats]
    fn message(&self) -> String {
        let SyntaxError { message } = self;
        format!("SyntaxError: {message}")
    }
}

/// E901
pub(crate) fn syntax_error(
    diagnostics: &mut Vec<Diagnostic>,
    parse_error: &ParseError,
    locator: &Locator,
) {
    let rest = locator.after(parse_error.offset);

    // Try to create a non-empty range so that the diagnostic can print a caret at the
    // right position. This requires that we retrieve the next character, if any, and take its length
    // to maintain char-boundaries.
    let len = rest
        .chars()
        .next()
        .map_or(TextSize::new(0), TextLen::text_len);

    diagnostics.push(Diagnostic::new(
        SyntaxError {
            message: format!("{}", DisplayParseErrorType::new(&parse_error.error)),
        },
        TextRange::at(parse_error.offset, len),
    ));
}
