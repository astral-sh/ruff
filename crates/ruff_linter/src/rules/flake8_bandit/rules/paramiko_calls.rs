use ruff_python_ast::Expr;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `paramiko` calls.
///
/// ## Why is this bad?
/// `paramiko` calls allow users to execute arbitrary shell commands on a
/// remote machine. If the inputs to these calls are not properly sanitized,
/// they can be vulnerable to shell injection attacks.
///
/// ## Example
/// ```python
/// import paramiko
///
/// client = paramiko.SSHClient()
/// client.exec_command("echo $HOME")
/// ```
///
/// ## References
/// - [Common Weakness Enumeration: CWE-78](https://cwe.mitre.org/data/definitions/78.html)
/// - [Paramiko documentation: `SSHClient.exec_command()`](https://docs.paramiko.org/en/stable/api/client.html#paramiko.client.SSHClient.exec_command)
#[derive(ViolationMetadata)]
pub(crate) struct ParamikoCall;

impl Violation for ParamikoCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Possible shell injection via Paramiko call; check inputs are properly sanitized"
            .to_string()
    }
}

/// S601
pub(crate) fn paramiko_call(checker: &Checker, func: &Expr) {
    if checker
        .semantic()
        .resolve_qualified_name(func)
        .is_some_and(|qualified_name| {
            matches!(qualified_name.segments(), ["paramiko", "exec_command"])
        })
    {
        checker.report_diagnostic(Diagnostic::new(ParamikoCall, func.range()));
    }
}
