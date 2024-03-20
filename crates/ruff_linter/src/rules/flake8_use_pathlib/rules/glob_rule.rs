use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};

/// ## What it does
/// Checks for the use of `glob` and `iglob`.
///
/// ## Why is this bad?
/// `pathlib` offers a high-level API for path manipulation, as compared to
/// the lower-level API offered by `os` and `glob`.
///
/// When possible, using `Path` object methods such as `Path.glob()` can
/// improve readability over their low-level counterparts (e.g.,
/// `glob.glob()`).
///
/// Note that `glob.glob` and `Path.glob` are not exact equivalents:
///
/// |                   | `glob`                                                                                                                                                             | `Path.glob`                                                                                                                                |
/// |-------------------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------|--------------------------------------------------------------------------------------------------------------------------------------------|
/// | Hidden files      | Excludes hidden files by default. From Python 3.11 onwards, the `include_hidden` keyword can be used to include hidden directories.                                   | Includes hidden files by default.                                                                                                          |
/// | Iterator          | `iglob` returns an iterator. Under the hood, `glob` simply converts the iterator to a list.                                                                        | `Path.glob` returns an iterator.                                                                                                           |
/// | Working directory | `glob` takes a `root_dir` keyword to set the current working directory.                                                                                            | `Path.rglob` can be used to return the relative path.                                                                                      |
/// | Globstar (`**`)   | `glob` requires the `recursive` flag to be set to `True` for the `**` pattern to match any files and zero or more directories, subdirectories, and symbolic links. | The `**` pattern in `Path.glob` means "this directory and all subdirectories, recursively". In other words, it enables recursive globbing. |
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
/// ```
///
/// ## References
/// - [Python documentation: `Path.glob`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.glob)
/// - [Python documentation: `Path.rglob`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.rglob)
/// - [Python documentation: `glob.glob`](https://docs.python.org/3/library/glob.html#glob.glob)
/// - [Python documentation: `glob.iglob`](https://docs.python.org/3/library/glob.html#glob.iglob)
#[violation]
pub struct Glob {
    pub function: String,
}

impl Violation for Glob {
    #[derive_message_formats]
    fn message(&self) -> String {
        let Glob { function } = self;
        format!("Replace `{function}` with `Path.glob` or `Path.rglob`")
    }
}
