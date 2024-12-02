use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, ViolationMetadata};

/// ## What it does
/// Checks for the use of `glob.glob()` and `glob.iglob()`.
///
/// ## Why is this bad?
/// `pathlib` offers a high-level API for path manipulation, as compared to
/// the lower-level API offered by `os` and `glob`.
///
/// When possible, using `Path` object methods such as `Path.glob()` can
/// improve readability over their low-level counterparts (e.g.,
/// `glob.glob()`).
///
/// Note that `glob.glob()` and `Path.glob()` are not exact equivalents:
///
/// |                   | `glob`-module functions                                                                                                                              | `Path.glob()`                                                                                                                                |
/// |-------------------|------------------------------------------------------------------------------------------------------------------------------------------------------|----------------------------------------------------------------------------------------------------------------------------------------------|
/// | Hidden files      | Hidden files are excluded by default. On Python 3.11+, the `include_hidden` keyword can be used to include hidden directories.                       | Includes hidden files by default.                                                                                                            |
/// | Eagerness         | `glob.iglob()` returns a lazy iterator. Under the hood, `glob.glob()` simply converts the iterator to a list.                                        | `Path.glob()` returns a lazy iterator.                                                                                                       |
/// | Working directory | `glob.glob()` and `glob.iglob()` take a `root_dir` keyword to set the current working directory.                                                     | `Path.rglob()` can be used to return the relative path.                                                                                      |
/// | Globstar (`**`)   | The `recursive` flag must be set to `True` for the `**` pattern to match any files and zero or more directories, subdirectories, and symbolic links. | The `**` pattern in `Path.glob()` means "this directory and all subdirectories, recursively". In other words, it enables recursive globbing. |
///
/// ## Example
/// ```python
/// import glob
/// import os
///
/// glob.glob(os.path.join("my_path", "requirements*.txt"))
/// ```
///
/// Use instead:
/// ```python
/// from pathlib import Path
///
/// Path("my_path").glob("requirements*.txt")
/// ```
///
/// ## Known issues
/// While using `pathlib` can improve the readability and type safety of your code,
/// it can be less performant than the lower-level alternatives that work directly with strings,
/// especially on older versions of Python.
///
/// ## References
/// - [Python documentation: `Path.glob`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.glob)
/// - [Python documentation: `Path.rglob`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.rglob)
/// - [Python documentation: `glob.glob`](https://docs.python.org/3/library/glob.html#glob.glob)
/// - [Python documentation: `glob.iglob`](https://docs.python.org/3/library/glob.html#glob.iglob)
/// - [PEP 428 – The pathlib module – object-oriented filesystem paths](https://peps.python.org/pep-0428/)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[derive(ViolationMetadata)]
pub(crate) struct Glob {
    pub function: String,
}

impl Violation for Glob {
    #[derive_message_formats]
    fn message(&self) -> String {
        let Glob { function } = self;
        format!("Replace `{function}` with `Path.glob` or `Path.rglob`")
    }
}
