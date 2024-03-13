use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::map_callable;
use ruff_python_ast::{Expr, ExprAttribute, ExprCall};
use ruff_python_semantic::analyze::typing;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for uses of policies disabling SSH verification in Paramiko.
///
/// ## Why is this bad?
/// By default, Paramiko checks the identity of the remote host when establishing
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
/// ssh_client.set_missing_host_key_policy(client.RejectPolicy)
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

    // Detect either, e.g., `paramiko.client.AutoAddPolicy` or `paramiko.client.AutoAddPolicy()`.
    if !checker
        .semantic()
        .resolve_qualified_name(map_callable(policy_argument))
        .is_some_and(|qualified_name| {
            matches!(
                qualified_name.segments(),
                ["paramiko", "client", "AutoAddPolicy" | "WarningPolicy"]
                    | ["paramiko", "AutoAddPolicy" | "WarningPolicy"]
            )
        })
    {
        return;
    }

    if typing::resolve_assignment(value, checker.semantic()).is_some_and(|qualified_name| {
        matches!(
            qualified_name.segments(),
            ["paramiko", "client", "SSHClient"] | ["paramiko", "SSHClient"]
        )
    }) {
        checker.diagnostics.push(Diagnostic::new(
            SSHNoHostKeyVerification,
            policy_argument.range(),
        ));
    }
}
