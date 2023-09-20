use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{Expr, ExprAttribute, ExprCall, Stmt, StmtAssign};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for uses of policies disabling SSH verification in Paramiko.
///
/// ## Why is this bad?
/// By default, Paramiko checks the identity of remote host when establishing
/// an SSH connection. Disabling the verification might lead to the client
/// connecting to a malicious host, without the client knowing.
///
/// ## Example
/// ```python
/// from paramiko import client
///
/// ssh_client = client.SSHClient()
/// ssh_client.set_missing_host_key_policy(client.AutoAddPolicy)
/// ```
///
/// Use instead:
/// ```python
/// from paramiko import client
///
/// ssh_client = client.SSHClient()
/// ssh_client.set_missing_host_key_policy()
/// ```
///
/// ## References
/// - [Paramiko documentation: set_missing_host_key_policy](https://docs.paramiko.org/en/latest/api/client.html#paramiko.client.SSHClient.set_missing_host_key_policy)
#[violation]
pub struct SSHNoHostKeyVerification;

impl Violation for SSHNoHostKeyVerification {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Paramiko call with policy set to automatically trust the unknown host key")
    }
}

/// S507
pub(crate) fn ssh_no_host_key_verification(checker: &mut Checker, call: &ExprCall) {
    let Expr::Attribute(ExprAttribute { attr, value, .. }) = call.func.as_ref() else {
        return;
    };

    if attr.as_str() != "set_missing_host_key_policy" {
        return;
    }

    let Some(policy_argument) = call.arguments.find_argument("policy", 0) else {
        return;
    };

    if !checker
        .semantic()
        .resolve_call_path(policy_argument)
        .is_some_and(|call_path| {
            matches!(
                call_path.as_slice(),
                ["paramiko", "client", "AutoAddPolicy" | "WarningPolicy"]
            )
        })
    {
        return;
    }

    let Expr::Name(name) = value.as_ref() else {
        return;
    };

    if let Some(binding_id) = checker.semantic().resolve_name(name) {
        if let Some(Stmt::Assign(StmtAssign { value, .. })) = checker
            .semantic()
            .binding(binding_id)
            .statement(checker.semantic())
        {
            if let Expr::Call(ExprCall { func, .. }) = value.as_ref() {
                if checker
                    .semantic()
                    .resolve_call_path(func)
                    .is_some_and(|call_path| {
                        matches!(call_path.as_slice(), ["paramiko", "client", "SSHClient"])
                    })
                {
                    checker.diagnostics.push(Diagnostic::new(
                        SSHNoHostKeyVerification,
                        policy_argument.range(),
                    ));
                }
            }
        }
    };
}
