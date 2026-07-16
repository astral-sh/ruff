use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast};
use ruff_python_semantic::analyze::function_type::{self, FunctionType};
use ruff_python_semantic::{Scope, ScopeKind};
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for default values on receiver parameters (e.g., `self`, `cls`) in method definitions.
///
/// ## Why is this bad?
/// Receiver parameters (`self`, `cls`, or any name used as the receiver) should not have default
/// values. In practice, these parameters are usually bound by the method binding protocol, so a
/// default value on a receiver parameter is almost
/// certainly a mistake and can lead to confusing behavior or runtime errors.
///
/// ## Example
///
/// ```python
/// class A:
///     def method(self=None): ...
///
///     @classmethod
///     def build(cls=None): ...
/// ```
///
/// Use instead:
///
/// ```python
/// class A:
///     def method(self): ...
///
///     @classmethod
///     def build(cls): ...
/// ```
///
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "NEXT_RUFF_VERSION")]
pub(crate) struct MethodReceiverDefault {
    receiver_kind: ReceiverKind,
}

#[derive(Debug, Clone, Copy)]
enum ReceiverKind {
    Instance,
    Class,
}

impl Violation for MethodReceiverDefault {
    #[derive_message_formats]
    fn message(&self) -> String {
        match self.receiver_kind {
            ReceiverKind::Instance => {
                "Instance receiver parameter should not have a default value".to_string()
            }
            ReceiverKind::Class => {
                "Class receiver parameter should not have a default value".to_string()
            }
        }
    }

    fn fix_title(&self) -> Option<String> {
        Some("Remove default value from receiver parameter".to_string())
    }
}

/// Determine the receiver kind for a function, if it has a receiver parameter.
fn receiver_kind(
    name: &str,
    decorator_list: &[ast::Decorator],
    parent_scope: &Scope,
    checker: &Checker,
) -> Option<ReceiverKind> {
    let function_kind = function_type::classify(
        name,
        decorator_list,
        parent_scope,
        checker.semantic(),
        &checker.settings().pep8_naming.classmethod_decorators,
        &checker.settings().pep8_naming.staticmethod_decorators,
    );

    match function_kind {
        FunctionType::StaticMethod => None,
        FunctionType::ClassMethod => Some(ReceiverKind::Class),
        FunctionType::NewMethod => Some(ReceiverKind::Class),
        FunctionType::Method if decorator_list.is_empty() => Some(ReceiverKind::Instance),
        FunctionType::Method => None,
        FunctionType::Function => None,
    }
}

/// RUF077 — Method receiver parameter should not have a default value
pub(crate) fn method_receiver_default(checker: &Checker, scope: &Scope) {
    let ScopeKind::Function(ast::StmtFunctionDef {
        name,
        parameters,
        decorator_list,
        ..
    }) = &scope.kind
    else {
        panic!("Expected ScopeKind::Function")
    };

    let semantic = checker.semantic();

    let Some(parent_scope) = semantic.first_non_type_parent_scope(scope) else {
        return;
    };

    let ScopeKind::Class(_) = parent_scope.kind else {
        return;
    };

    // Determine receiver kind
    let Some(receiver_kind) = receiver_kind(name.as_str(), decorator_list, parent_scope, checker)
    else {
        return;
    };

    // Get the first parameter (the receiver)
    let Some(first_param) = parameters
        .posonlyargs
        .first()
        .or_else(|| parameters.args.first())
    else {
        return;
    };

    // Check if the receiver parameter has a default value
    if let Some(default_expr) = &first_param.default {
        let diagnostic = MethodReceiverDefault { receiver_kind };

        checker.report_diagnostic(diagnostic, default_expr.range());
    }
}
