use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, Expr, Stmt};
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
#[violation_metadata(stable_since = "0.15.20")]
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
    // Check if it's a static method
    if is_staticmethod(function, checker) {
        return None;
    }

    // Check if it's a classmethod
    if is_classmethod(function, checker) {
        return Some(ReceiverKind::Class);
    }

    // Check if it's __new__ (class receiver)
    if function.name.as_str() == "__new__" {
        return Some(ReceiverKind::Class);
    }

    // Default: instance receiver
    Some(ReceiverKind::Instance)
}

/// Check if a function is decorated with @staticmethod
fn is_staticmethod(function: &ast::StmtFunctionDef, checker: &Checker) -> bool {
    function
        .decorator_list
        .iter()
        .any(|decorator| is_name_or_attr(&decorator.expression, "staticmethod", checker))
}

/// Check if a function is decorated with @classmethod
fn is_classmethod(function: &ast::StmtFunctionDef, checker: &Checker) -> bool {
    function
        .decorator_list
        .iter()
        .any(|decorator| is_name_or_attr(&decorator.expression, "classmethod", checker))
}

/// Check if an expression is a name or attribute matching the given string
fn is_name_or_attr(expr: &Expr, name: &str, _checker: &Checker) -> bool {
    match expr {
        Expr::Name(ast::ExprName { id, .. }) => id == name,
        Expr::Attribute(ast::ExprAttribute { attr, .. }) => attr == name,
        _ => false,
    }
}

/// RUF077 — Method receiver parameter should not have a default value
pub(crate) fn method_receiver_default(checker: &Checker, function: &ast::StmtFunctionDef) {
    // Only check functions directly in a class body
    let Some(Stmt::ClassDef(_)) = checker.semantic().current_statement_parent() else {
        return;
    };

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
