use ruff_python_ast::Expr;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
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
#[violation]
pub struct ParamikoCall;

impl Violation for ParamikoCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Possible shell injection via Paramiko call; check inputs are properly sanitized")
    }
}

/// S601
pub(crate) fn paramiko_call(checker: &mut Checker, func: &Expr) {
    if checker
        .semantic()
        .resolve_call_path(func)
        .is_some_and(|call_path| matches!(call_path.as_slice(), ["paramiko", "exec_command"]))
    {
        checker
            .diagnostics
            .push(Diagnostic::new(ParamikoCall, func.range()));
    }
}
