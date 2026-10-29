use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::helpers::map_callable;
use ruff_python_ast::name::QualifiedName;
use ruff_python_ast::{self as ast, Expr, ExprAttribute, ExprCall, Stmt};
use ruff_python_semantic::analyze::class::any_qualified_base_class;
use ruff_python_semantic::analyze::{function_type, typing};
use ruff_python_semantic::{Binding, BindingKind, ScopeKind, SemanticModel};
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;

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
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.2.0", category = Category::Security)]
pub(crate) struct SSHNoHostKeyVerification;

impl Violation for SSHNoHostKeyVerification {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Paramiko call with policy set to automatically trust the unknown host key".to_string()
    }
}

/// S507
pub(crate) fn ssh_no_host_key_verification(checker: &Checker, call: &ExprCall) {
    let Expr::Attribute(ExprAttribute { attr, value, .. }) = call.func.as_ref() else {
        return;
    };

    if attr.as_str() != "set_missing_host_key_policy" {
        return;
    }

    let Some(policy_argument) = call.arguments.find_argument_value("policy", 0) else {
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

    if is_ssh_client(value, checker) {
        checker.report_diagnostic(SSHNoHostKeyVerification, policy_argument.range());
    }
}

fn is_ssh_client_name(qualified_name: &QualifiedName) -> bool {
    matches!(
        qualified_name.segments(),
        ["paramiko", "client", "SSHClient"] | ["paramiko", "SSHClient"]
    )
}

/// Returns `true` if `class_def` inherits from `paramiko.SSHClient`, directly or through other
/// classes defined in the same file.
fn is_ssh_client_subclass(class_def: &ast::StmtClassDef, semantic: &SemanticModel) -> bool {
    any_qualified_base_class(class_def, semantic, |qualified_name| {
        is_ssh_client_name(&qualified_name)
    })
}

/// Returns `true` if `value` is likely an instance of `paramiko.SSHClient` or of a subclass of it.
///
/// For example:
/// ```python
/// ssh_client = paramiko.SSHClient()
///
/// class Client(paramiko.SSHClient):
///     def connect_insecure(self):
///         self.set_missing_host_key_policy(...)  # `self` is an `SSHClient`
///
/// client = Client()  # `client` is an `SSHClient`
/// ```
fn is_ssh_client(value: &Expr, checker: &Checker) -> bool {
    let semantic = checker.semantic();

    if typing::resolve_assignment(value, semantic)
        .is_some_and(|qualified_name| is_ssh_client_name(&qualified_name))
    {
        return true;
    }

    let Expr::Name(name) = value else {
        return false;
    };
    let Some(binding_id) = semantic.resolve_name(name) else {
        return false;
    };
    let binding = semantic.binding(binding_id);

    // Ex) `self.set_missing_host_key_policy(...)` in a method of a subclass of `SSHClient`
    if let Some(class_def) = method_receiver_class(binding, checker) {
        return is_ssh_client_subclass(class_def, semantic);
    }

    // Ex) `client = Client()`, where `Client` is a subclass of `SSHClient` in the same file
    let Some(Expr::Call(ExprCall { func, .. })) = typing::find_binding_value(binding, semantic)
    else {
        return false;
    };
    let Some(class_binding_id) = semantic.lookup_attribute(func) else {
        return false;
    };
    let Some(class_scope_id) = semantic
        .binding(class_binding_id)
        .kind
        .as_class_definition()
    else {
        return false;
    };
    let ScopeKind::Class(class_def) = semantic.scopes[*class_scope_id].kind else {
        return false;
    };
    is_ssh_client_subclass(class_def, semantic)
}

/// If `binding` is the first parameter of an instance method (usually `self`), return the
/// class the method is defined in.
fn method_receiver_class<'a>(
    binding: &Binding,
    checker: &'a Checker,
) -> Option<&'a ast::StmtClassDef> {
    let semantic = checker.semantic();

    if !matches!(binding.kind, BindingKind::Argument) {
        return None;
    }

    let Some(Stmt::FunctionDef(function)) = binding.statement(semantic) else {
        return None;
    };

    let first_parameter = function
        .parameters
        .posonlyargs
        .first()
        .or_else(|| function.parameters.args.first())?;
    if binding.range != first_parameter.parameter.name.range() {
        return None;
    }

    let parent_scope = semantic.first_non_type_parent_scope(&semantic.scopes[binding.scope])?;
    let ScopeKind::Class(class_def) = parent_scope.kind else {
        return None;
    };

    let function_type = function_type::classify(
        &function.name,
        &function.decorator_list,
        parent_scope,
        semantic,
        &checker.settings().pep8_naming.classmethod_decorators,
        &checker.settings().pep8_naming.staticmethod_decorators,
    );
    matches!(function_type, function_type::FunctionType::Method).then_some(class_def)
}
