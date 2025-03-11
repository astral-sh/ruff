use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, ViolationMetadata};

/// ## What it does
/// Checks for uses of `os.path.getmtime`.
///
/// ## Why is this bad?
/// `pathlib` offers a high-level API for path manipulation, as compared to
/// the lower-level API offered by `os.path`.
///
/// When possible, using `Path` object methods such as `Path.stat()` can
/// improve readability over the `os.path` module's counterparts (e.g.,
/// `os.path.getmtime()`).
///
/// ## Example
/// ```python
/// import os
///
/// os.path.getmtime(__file__)
/// ```
///
/// Use instead:
/// ```python
/// from pathlib import Path
///
/// Path(__file__).stat().st_mtime
/// ```
///
/// ## Known issues
/// While using `pathlib` can improve the readability and type safety of your code,
/// it can be less performant than the lower-level alternatives that work directly with strings,
/// especially on older versions of Python.
///
/// ## References
/// - [Python documentation: `Path.stat`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.stat)
/// - [Python documentation: `os.path.getmtime`](https://docs.python.org/3/library/os.path.html#os.path.getmtime)
/// - [PEP 428 – The pathlib module – object-oriented filesystem paths](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#correspondence-to-tools-in-the-os-module)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[derive(ViolationMetadata)]
pub(crate) struct OsPathGetmtime;

impl Violation for OsPathGetmtime {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`os.path.getmtime` should be replaced by `Path.stat().st_mtime`".to_string()
    }
}
