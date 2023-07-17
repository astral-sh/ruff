use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};

/// PTH204

/// ## What it does
/// Detects the use of `os.path.getatime`.
///
/// ## Why is this bad?
/// `pathlib` offers high-level path manipulations of paths, `os` offers low-level manipulation of paths.
/// Where possible, using `Path` object methods such as `Path.stat()` improve readability over their `os`
/// counterparts such as `os.path.getmtime()`.
///
/// There are situations where creating many `Path` object causes overhead. `os` functions therefore remain
/// preferable in heavy loops and data structures storing paths (e.g. pandas).
///
/// ## Examples
/// ```python
/// os.path.getmtime(__file__)
/// ```
///
/// Use instead:
/// ```python
/// Path(__file__).stat().st_mtime
/// ```
///
/// ## References
/// - [Python documentation: `Path.stat`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.stat)
/// - [Python documentation: `os.path.getmtime`](https://docs.python.org/3/library/os.path.html#os.path.getmtime)
/// - [PEP 428](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#correspondence-to-tools-in-the-os-module)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[violation]
pub struct OsPathGetmtime;

impl Violation for OsPathGetmtime {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.path.getmtime` should be replaced by `Path.stat().st_mtime`")
    }
}
