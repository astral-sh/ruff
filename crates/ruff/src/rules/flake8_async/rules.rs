use rustpython_parser::ast::{Expr, ExprKind};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::call_path::collect_call_path;
use ruff_python_semantic::scope::{FunctionDef, ScopeKind};

use crate::checkers::ast::Checker;
use crate::registry::{AsRule};

// TODO: Fix docstrings and improve messages

/// ## What it does
/// Checks that async functions do not contain a blocking HTTP call
///
/// ## Why is this bad?
/// TODO
///
/// ## Example
/// TODO
///
/// Use instead:
/// TODO
#[violation]
pub struct BlockingHttpCallInsideAsyncDef;

impl Violation for BlockingHttpCallInsideAsyncDef {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Async functions should not call a blocking HTTP method")
    }
}

/// ASY100
// TODO: Implement
pub fn blocking_http_call_inside_async_def(checker: &mut Checker, func: &Expr) {
    let diagnostic = Diagnostic::new(BlockingHttpCallInsideAsyncDef, func.range());

    if !checker.settings.rules.enabled(diagnostic.kind.rule()) {
        return;
    }

    checker.diagnostics.push(diagnostic);
}


/// ## What it does
/// Checks that async functions do not contain a call to `open`, `time.sleep` or `subprocess`
/// methods
///
/// ## Why is this bad?
/// TODO
///
/// ## Example
/// TODO
///
/// Use instead:
/// TODO
#[violation]
pub struct OpenSleepOrSubprocessInsideAsyncDef;

impl Violation for OpenSleepOrSubprocessInsideAsyncDef {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Async functions should not contain a call to `open`, `time.sleep` or any \
             subprocess method"
        )
    }
}

/// ASY101
// TODO: Implement
pub fn open_sleep_or_subprocess_inside_async_def(checker: &mut Checker, expr: &Expr) {
    if checker
        .ctx
        .scopes()
        .find_map(|scope| {
            if let ScopeKind::Function(FunctionDef { async_, .. }) = &scope.kind {
                Some(*async_)
            } else {
                None
            }
        })
        .unwrap_or(false) {
        if let ExprKind::Call { func, .. } = &expr.node {
            if let Some(call_path) = collect_call_path(func) {
                if call_path.as_slice() == ["time", "sleep"]
                {
                    checker.diagnostics.push(Diagnostic::new(OpenSleepOrSubprocessInsideAsyncDef, expr.range));
                }
            }
        }
    }
}

/// ## What it does
/// Checks that async functions do not contain a call to an unsafe `os` method
///
/// ## Why is this bad?
/// TODO
///
/// ## Example
/// TODO
///
/// Use instead:
/// TODO
#[violation]
pub struct UnsafeOsMethodInsideAsyncDef;

impl Violation for UnsafeOsMethodInsideAsyncDef {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Async functions should not contain a call to unsafe `os` methods")
    }
}

/// ASY102
// TODO: Implement
pub fn unsafe_os_method_inside_async_def(checker: &mut Checker, func: &Expr) {
    let diagnostic = Diagnostic::new(UnsafeOsMethodInsideAsyncDef, func.range());

    if !checker.settings.rules.enabled(diagnostic.kind.rule()) {
        return;
    }

    checker.diagnostics.push(diagnostic);
}
