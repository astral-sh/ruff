use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, Expr};
use ruff_python_semantic::analyze::typing;
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::preview::is_s601_client_calls_enabled;

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
/// ## Preview
/// When [preview] is enabled, this rule also flags `exec_command` calls on
/// `paramiko.SSHClient` and `paramiko.Channel` objects, as in the example
/// above. The object is recognized when it is constructed directly, assigned
/// to a variable, bound in a `with` statement, or annotated with one of those
/// classes. Outside preview, only calls to `paramiko.exec_command` are
/// flagged.
///
/// [preview]: https://docs.astral.sh/ruff/preview/
///
/// ## References
/// - [Common Weakness Enumeration: CWE-78](https://cwe.mitre.org/data/definitions/78.html)
/// - [Paramiko documentation: `SSHClient.exec_command()`](https://docs.paramiko.org/en/stable/api/client.html#paramiko.client.SSHClient.exec_command)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.270", category = Category::Security)]
pub(crate) struct ParamikoCall;

impl Violation for ParamikoCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Possible shell injection via Paramiko call; check inputs are properly sanitized"
            .to_string()
    }
}

/// S601
pub(crate) fn paramiko_call(checker: &Checker, call: &ast::ExprCall) {
    let semantic = checker.semantic();

    if semantic
        .resolve_qualified_name(&call.func)
        .is_some_and(|qualified_name| {
            matches!(qualified_name.segments(), ["paramiko", "exec_command"])
        })
    {
        checker.report_diagnostic(ParamikoCall, call.func.range());
        return;
    }

    if !is_s601_client_calls_enabled(checker.settings()) {
        return;
    }

    // `paramiko` runs remote commands through methods on client objects, so
    // look for `exec_command` called on an `SSHClient` or a `Channel`:
    //
    // ```python
    // client = paramiko.SSHClient()
    // client.exec_command("echo $HOME")
    // ```
    let Expr::Attribute(ast::ExprAttribute { value, attr, .. }) = call.func.as_ref() else {
        return;
    };

    if attr.as_str() != "exec_command" {
        return;
    }

    let is_command_runner = match value.as_ref() {
        Expr::Name(name) => semantic.only_binding(name).is_some_and(|binding_id| {
            typing::is_paramiko_command_runner(semantic.binding(binding_id), semantic)
        }),
        expr => typing::is_paramiko_command_runner_expr(expr, semantic),
    };

    if is_command_runner {
        checker.report_diagnostic(ParamikoCall, call.func.range());
    }
}
