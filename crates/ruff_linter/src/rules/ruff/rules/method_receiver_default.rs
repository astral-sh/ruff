use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, Stmt};
use ruff_python_semantic::analyze::function_type::{self, FunctionType};
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for default values on receiver parameters (e.g., `self`, `cls`) in method definitions.
///
/// ## Why is this bad?
/// Receiver parameters (`self`, `cls`, or any name used as the receiver) should not have default
/// values. In practice, these parameters are always bound by the method binding protocol and
/// cannot be omitted when calling the method. A default value on a receiver parameter is almost
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
fn classify_receiver_kind(
    function: &ast::StmtFunctionDef,
    checker: &Checker,
) -> Option<ReceiverKind> {
    let function_kind = function_type::classify(
        function.name.as_str(),
        &function.decorator_list,
        checker.semantic().current_scope(),
        checker.semantic(),
        &checker.settings().pep8_naming.classmethod_decorators,
        &checker.settings().pep8_naming.staticmethod_decorators,
    );

    match function_kind {
        FunctionType::StaticMethod => None,
        FunctionType::ClassMethod => Some(ReceiverKind::Class),
        FunctionType::NewMethod => Some(ReceiverKind::Class),
        FunctionType::Method => Some(ReceiverKind::Instance),
        _ => None,
    }
}

/// RUF077 — Method receiver parameter should not have a default value
pub(crate) fn method_receiver_default(checker: &Checker, function: &ast::StmtFunctionDef) {
    // Only check functions directly in a class body
    let Some(Stmt::ClassDef(_)) = checker.semantic().current_statement_parent() else {
        return;
    };

    // Conservatively bail out if there are decorators beyond the standard ones we handle
    // This includes @override and other custom decorators that may indicate inherited signatures
    let classmethod_decorators = &checker.settings().pep8_naming.classmethod_decorators;
    let staticmethod_decorators = &checker.settings().pep8_naming.staticmethod_decorators;

    for decorator in &function.decorator_list {
        let decorator_name = match decorator {
            ast::Decorator {
                expression: ast::Expr::Name(name),
                ..
            } => Some(name.id.as_str()),
            ast::Decorator {
                expression: ast::Expr::Attribute(attr),
                ..
            } => Some(attr.attr.as_str()),
            _ => None,
        };

        if let Some(name) = decorator_name {
            // Skip standard decorators we handle
            if name == "classmethod"
                || name == "staticmethod"
                || classmethod_decorators.contains(&name.to_string())
                || staticmethod_decorators.contains(&name.to_string())
                || name == "property"
                || name == "override"
            {
                continue;
            }
            // If we encounter any other decorator, bail out conservatively
            return;
        }
    }

    // Determine receiver kind
    let Some(receiver_kind) = classify_receiver_kind(function, checker) else {
        return;
    };

    // Get the first parameter (the receiver)
    let Some(first_param) = function
        .parameters
        .posonlyargs
        .first()
        .or_else(|| function.parameters.args.first())
    else {
        return;
    };

    // Check if the receiver parameter has a default value
    if first_param.default.is_some() {
        let diagnostic = MethodReceiverDefault { receiver_kind };

        // Report at the default value location if available, otherwise at the parameter
        if let Some(default_expr) = &first_param.default {
            checker.report_diagnostic(diagnostic, default_expr.range());
        } else {
            checker.report_diagnostic(diagnostic, first_param.range());
        }
    }
}
