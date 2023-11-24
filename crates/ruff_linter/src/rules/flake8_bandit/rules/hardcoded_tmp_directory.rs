use ruff_python_ast::{self as ast, Expr};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for the use of hardcoded temporary file or directory paths.
///
/// ## Why is this bad?
/// The use of hardcoded paths for temporary files can be insecure. If an
/// attacker discovers the location of a hardcoded path, they can replace the
/// contents of the file or directory with a malicious payload.
///
/// Other programs may also read or write contents to these hardcoded paths,
/// causing unexpected behavior.
///
/// ## Example
/// ```python
/// with open("/tmp/foo.txt", "w") as file:
///     ...
/// ```
///
/// Use instead:
/// ```python
/// import tempfile
///
/// with tempfile.NamedTemporaryFile() as file:
///     ...
/// ```
///
/// ## References
/// - [Common Weakness Enumeration: CWE-377](https://cwe.mitre.org/data/definitions/377.html)
/// - [Common Weakness Enumeration: CWE-379](https://cwe.mitre.org/data/definitions/379.html)
/// - [Python documentation: `tempfile`](https://docs.python.org/3/library/tempfile.html)
#[violation]
pub struct HardcodedTempFile {
    string: String,
}

impl Violation for HardcodedTempFile {
    #[derive_message_formats]
    fn message(&self) -> String {
        let HardcodedTempFile { string } = self;
        format!(
            "Probable insecure usage of temporary file or directory: \"{}\"",
            string.escape_debug()
        )
    }
}

/// S108
pub(crate) fn hardcoded_tmp_directory(checker: &mut Checker, string: &ast::ExprStringLiteral) {
    if !checker
        .settings
        .flake8_bandit
        .hardcoded_tmp_directory
        .iter()
        .any(|prefix| string.value.starts_with(prefix))
    {
        return;
    }

    if let Some(Expr::Call(ast::ExprCall { func, .. })) =
        checker.semantic().current_expression_parent()
    {
        if checker
            .semantic()
            .resolve_call_path(func)
            .is_some_and(|call_path| matches!(call_path.as_slice(), ["tempfile", ..]))
        {
            return;
        }
    }

    checker.diagnostics.push(Diagnostic::new(
        HardcodedTempFile {
            string: string.value.to_string(),
        },
        string.range,
    ));
}
