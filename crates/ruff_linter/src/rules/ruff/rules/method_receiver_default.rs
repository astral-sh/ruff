use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::token::parenthesized_range;
use ruff_python_ast::{self as ast};
use ruff_python_semantic::analyze::function_type::{self, FunctionType, is_class_method};
use ruff_python_semantic::{Scope, ScopeKind};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::{Edit, Fix, FixAvailability, Violation};

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
/// ## Fix safety
/// This fix is always marked as unsafe. Removing the default changes behavior for a caller that
/// invokes the method directly through the class rather than through an instance (e.g.
/// `A.method()` instead of `A().method()`), relying on the receiver parameter's default value.
///
/// ## Known limitations
/// To avoid false positives, this rule only flags an undecorated method, an undecorated
/// `__new__`, or a method whose sole decorator is the one that makes it a `@classmethod`. A
/// method with any other decorator (including common ones like `@property`, `@x.setter`, or
/// `@typing.override`) is not flagged, even though such decorators typically leave the receiver
/// binding unchanged, because an arbitrary decorator could alter it.
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "NEXT_RUFF_VERSION")]
pub(crate) struct MethodReceiverDefault;

impl Violation for MethodReceiverDefault {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Always;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Receiver parameter should not have a default value".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Remove default value from receiver parameter".to_string())
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
        return;
    };

    let semantic = checker.semantic();

    let Some(parent_scope) = semantic.first_non_type_parent_scope(scope) else {
        return;
    };

    let ScopeKind::Class(_) = parent_scope.kind else {
        return;
    };

    // Determine whether this function has a bound receiver parameter.
    if !has_receiver_parameter(name.as_str(), decorator_list, parent_scope, checker) {
        return;
    }

    // Get the first parameter (the receiver)
    let Some(first_param) = parameters
        .posonlyargs
        .first()
        .or_else(|| parameters.args.first())
    else {
        return;
    };

    // Check if the receiver parameter has a default value
    let Some(default_expr) = &first_param.default else {
        return;
    };

    // Account for a parenthesized default (e.g. `self=(None)`) so the fix removes the
    // parentheses along with the default value.
    let default_range = parenthesized_range(
        default_expr.as_ref().into(),
        first_param.into(),
        checker.tokens(),
    )
    .unwrap_or(default_expr.range());

    let edit = Edit::deletion(first_param.parameter.end(), default_range.end());

    checker
        .report_diagnostic(MethodReceiverDefault, default_expr.range())
        .set_fix(Fix::unsafe_edit(edit));
}

/// Determine whether a function has a bound receiver parameter.
fn has_receiver_parameter(
    name: &str,
    decorator_list: &[ast::Decorator],
    parent_scope: &Scope,
    checker: &Checker,
) -> bool {
    let semantic = checker.semantic();

    let function_kind = function_type::classify(
        name,
        decorator_list,
        parent_scope,
        semantic,
        &checker.settings().pep8_naming.classmethod_decorators,
        &checker.settings().pep8_naming.staticmethod_decorators,
    );

    match function_kind {
        // Trust the classification when there are no decorators at all (this is how an implicit
        // classmethod like `__init_subclass__` is classified) or when there is exactly one
        // decorator and it's the one that makes the function a classmethod. Otherwise, an
        // unrelated decorator (`@some_decorator`) or a stacked one alongside `@classmethod` could
        // change how the receiver is bound, so we bail out rather than risk a false positive —
        // even for an implicit classmethod with an unrelated decorator attached.
        FunctionType::ClassMethod => match decorator_list {
            [] => true,
            [decorator]
                if is_class_method(
                    decorator,
                    semantic,
                    &checker.settings().pep8_naming.classmethod_decorators,
                ) =>
            {
                true
            }
            _ => false,
        },
        FunctionType::NewMethod if decorator_list.is_empty() => true,
        FunctionType::Method if decorator_list.is_empty() => true,
        FunctionType::Function => false,
        _ => false,
    }
}
