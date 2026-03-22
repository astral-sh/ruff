use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast as ast;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;
use crate::{Edit, Fix, FixAvailability, Violation};

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
/// Note that `os.path.commonprefix` is valid for non-path string
/// comparisons (e.g., finding a common prefix among version numbers
/// or identifiers). In such cases, this rule should be ignored.
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
/// ## Fix safety
///
/// This fix is marked as unsafe because `os.path.commonprefix` and
/// `os.path.commonpath` have different semantics:
///
/// - `commonprefix` performs a character-by-character string comparison
///   and returns the longest common string prefix.
/// - `commonpath` compares path components and returns the longest common
///   path prefix.
///
/// If you are intentionally using `commonprefix` for non-path string
/// comparisons (e.g., finding a common prefix among arbitrary strings
/// like version numbers or identifiers), ignore this rule using
/// `# noqa: RUF071` or the [`lint.ruff.per-file-ignores`] configuration
/// option.
///
/// For example:
///
/// ```python
/// import os
///
/// # commonprefix works on non-path strings
/// os.path.commonprefix(["12345", "12378"])  # "123"
/// os.path.commonpath(["12345", "12378"])    # raises ValueError
/// ```
///
/// ## References
/// - [Python documentation: `os.path.commonprefix`](https://docs.python.org/3/library/os.path.html#os.path.commonprefix)
/// - [Python documentation: `os.path.commonpath`](https://docs.python.org/3/library/os.path.html#os.path.commonpath)
/// - [Why `os.path.commonprefix` is deprecated](https://sethmlarson.dev/deprecate-confusing-apis-like-os-path-commonprefix)
/// - [CPython deprecation issue](https://github.com/python/cpython/issues/144347)
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "0.15.6")]
pub(crate) struct OsPathCommonprefix;

impl Violation for OsPathCommonprefix {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

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

    diagnostic.try_set_fix(|| {
        let (import_edit, binding) = checker.importer().get_or_import_symbol(
            &ImportRequest::import_from("os.path", "commonpath"),
            call.func.start(),
            checker.semantic(),
        )?;
        let reference_edit = Edit::range_replacement(binding, call.func.range());
        Ok(Fix::unsafe_edits(import_edit, [reference_edit]))
    });
}
