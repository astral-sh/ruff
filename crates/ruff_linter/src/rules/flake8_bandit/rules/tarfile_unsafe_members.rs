use crate::checkers::ast::Checker;
use ruff_diagnostics::Diagnostic;
use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast};
use ruff_text_size::Ranged;

/// ## What it does
/// Checks for uses of `tarfile.extractall`.
///
/// ## Why is this bad?
///
/// Extracting archives from untrusted sources without prior inspection is
/// a security risk, as maliciously crafted archives may contain files that
/// will be written outside of the target directory. For example, the archive
/// could include files with absolute paths (e.g., `/etc/passwd`), or relative
/// paths with parent directory references (e.g., `../etc/passwd`).
///
/// On Python 3.12 and later, use `filter='data'` to prevent the most dangerous
/// security issues (see: [PEP 706]). On earlier versions, set the `members`
/// argument to a trusted subset of the archive's members.
///
/// ## Example
/// ```python
/// import tarfile
/// import tempfile
///
/// tar = tarfile.open(filename)
/// tar.extractall(path=tempfile.mkdtemp())
/// tar.close()
/// ```
///
/// ## References
/// - [Common Weakness Enumeration: CWE-22](https://cwe.mitre.org/data/definitions/22.html)
/// - [Python Documentation: `TarFile.extractall`](https://docs.python.org/3/library/tarfile.html#tarfile.TarFile.extractall)
/// - [Python Documentation: Extraction filters](https://docs.python.org/3/library/tarfile.html#tarfile-extraction-filter)
///
/// [PEP 706]: https://peps.python.org/pep-0706/#backporting-forward-compatibility
#[violation]
pub struct TarfileUnsafeMembers;

impl Violation for TarfileUnsafeMembers {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Uses of `tarfile.extractall()`")
    }
}

/// S202
pub(crate) fn tarfile_unsafe_members(checker: &mut Checker, call: &ast::ExprCall) {
    if !call
        .func
        .as_attribute_expr()
        .is_some_and(|attr| attr.attr.as_str() == "extractall")
    {
        return;
    }

    if call
        .arguments
        .find_keyword("filter")
        .and_then(|keyword| keyword.value.as_string_literal_expr())
        .is_some_and(|value| matches!(value.value.to_str(), "data" | "tar"))
    {
        return;
    }

    if !checker.semantic().seen(&["tarfile"]) {
        return;
    }

    checker
        .diagnostics
        .push(Diagnostic::new(TarfileUnsafeMembers, call.func.range()));
}
