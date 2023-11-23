use crate::checkers::ast::Checker;
use ruff_diagnostics::Diagnostic;
use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast};
use ruff_text_size::Ranged;

/// ## What it does
/// Checks for uses of ``tarfile.extractall()`.
///
/// ## Why is this bad?
/// Use ``tarfile.extractall(members=function_name)`` and define a function
/// that will inspect each member. Discard files that contain a directory
/// traversal sequences such as ``../`` or ``\..`` along with all special filetypes
/// unless you explicitly need them.
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
/// - [Python Documentation: tarfile](https://docs.python.org/3/library/tarfile.html#tarfile.TarFile.extractall)
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
    if checker.semantic().seen(&["tarfile"])
        && call
            .func
            .as_attribute_expr()
            .is_some_and(|attr| attr.attr.as_str() == "extractall")
    {
        checker
            .diagnostics
            .push(Diagnostic::new(TarfileUnsafeMembers, call.func.range()));
    }
}
