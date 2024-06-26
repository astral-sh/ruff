use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};

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
