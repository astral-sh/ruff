use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};

/// ## What it does
/// Checks for the use of `glob`.
///
/// ## Why is this bad?
/// `pathlib` offers a high-level API for path manipulation, as compared to
/// the lower-level API offered by `os` and `glob`.
///
/// When possible, using `Path` object methods such as `Path.stat()` can
/// improve readability over their low-level counterparts (e.g.,
/// `glob.glob()`).
///
/// ## Example
/// ```python
/// import glob
/// import os
///
/// glob.glob(os.path.join(path, "requirements*.txt"))
/// ```
///
/// Use instead:
/// ```python
/// from pathlib import Path
///
/// Path(path).glob("requirements*.txt")
///
/// ## References
/// - [Python documentation: `Path.glob`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.glob)
/// - [Python documentation: `glob.glob`](https://docs.python.org/3/library/glob.html#glob.glob)
/// ```
#[violation]
pub struct Glob;

impl Violation for Glob {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Replace `glob` with `Path.glob`")
    }
}
