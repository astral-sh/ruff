use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, ViolationMetadata};

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
#[derive(ViolationMetadata)]
pub struct IOError {
    pub message: String,
}

/// E902
impl Violation for IOError {
    // The format message is used by the `derive_message_formats` macro.
    #![allow(clippy::useless_format)]
    #[derive_message_formats]
    fn message(&self) -> String {
        let IOError { message } = self;
        format!("{message}")
    }
}

/// ## Removed
/// This rule has been removed. Syntax errors will
/// always be shown regardless of whether this rule is selected or not.
///
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
#[derive(ViolationMetadata)]
#[deprecated(note = "E999 has been removed")]
pub(crate) struct SyntaxError;

#[allow(deprecated)]
impl Violation for SyntaxError {
    fn message(&self) -> String {
        unreachable!("E999 has been removed")
    }

    fn message_formats() -> &'static [&'static str] {
        &["SyntaxError"]
    }
}
