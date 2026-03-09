use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast as ast;
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for uses of `os.path.commonprefix`.
///
/// ## Why is this bad?
/// `os.path.commonprefix` performs a character-by-character string
/// comparison rather than comparing path components. This leads to
/// incorrect results when paths share a common string prefix that
/// is not a valid path component.
///
/// `os.path.commonpath` correctly compares path components.
///
/// `os.path.commonprefix` is deprecated as of Python 3.15.
///
/// ## Example
/// ```python
/// import os
///
/// # Returns "/usr/l" — not a valid directory!
/// os.path.commonprefix(["/usr/lib", "/usr/local/lib"])
/// ```
///
/// Use instead:
/// ```python
/// import os
///
/// # Returns "/usr" — correct common path
/// os.path.commonpath(["/usr/lib", "/usr/local/lib"])
/// ```
///
/// ## References
/// - [Python documentation: `os.path.commonprefix`](https://docs.python.org/3/library/os.path.html#os.path.commonprefix)
/// - [Python documentation: `os.path.commonpath`](https://docs.python.org/3/library/os.path.html#os.path.commonpath)
/// - [Why `os.path.commonprefix` is deprecated](https://sethmlarson.dev/deprecate-confusing-apis-like-os-path-commonprefix)
/// - [CPython deprecation issue](https://github.com/python/cpython/issues/144347)
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "NEXT_RUFF_VERSION")]
pub(crate) struct OsPathCommonprefix;

impl Violation for OsPathCommonprefix {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`os.path.commonprefix()` compares strings character-by-character".to_string()
    }
    
    fn fix_title(&self) -> Option<String> {
        Some("Use `os.path.commonpath()` to compare path components".to_string())
    }
}

/// RUF071
pub(crate) fn os_path_commonprefix(checker: &Checker, call: &ast::ExprCall, segments: &[&str]) {
    if segments != ["os", "path", "commonprefix"] {
        return;
    }
    let mut diagnostic = checker.report_diagnostic(OsPathCommonprefix, call.func.range());
    diagnostic.add_primary_tag(ruff_db::diagnostic::DiagnosticTag::Deprecated);
}
