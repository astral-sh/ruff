use ruff_python_ast::{
    AtomicNodeIndex, Expr, ExprAwait, ExprCall, ExprName, Stmt, StmtAssign, StmtExpr,
    StmtFunctionDef,
};
use ruff_text_size::{Ranged, TextRange};

use crate::{Edit, Fix, FixAvailability, Violation};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_semantic::SemanticModel;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for coroutines that are not awaited. This rule is only active in async contexts.
///
/// ## Why is this bad?
/// Coroutines are not executed until they are awaited. If a coroutine is not awaited, it will
/// not be executed, and the program will not behave as expected. This is a common mistake when
/// using `asyncio.sleep` instead of `await asyncio.sleep`.
///
/// Python's asyncio runtime will emit a warning when a coroutine is not awaited.
///
/// ## Examples
/// ```python
/// async def foo():
///     pass
///
///
/// async def bar():
///     foo()
/// ```
///
/// Use instead:
/// ```python
/// async def foo():
///     pass
///
///
/// async def bar():
///     await foo()
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct MissingAwaitForCoroutine;

impl Violation for MissingAwaitForCoroutine {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Coroutine is not awaited".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Coroutine is not awaited".to_string())
    }
}

/// RUF065
pub(crate) fn missing_await_for_coroutine(checker: &Checker, call: &ExprCall) {
    // Only check for missing await in async context
    if !checker.semantic().in_async_context() {
        return;
    }

    // Try to detect possible scenarios where await is missing and ignore other cases
    // For example, if the call is not a direct child of an statement expression or assignment statement
    // then it's not reliable to determine if await is missing.
    // User might return coroutine object from a function or pass it as an argument
    if !possibly_missing_await(call, checker.semantic()) {
        return;
    }

    let is_awaitable = is_awaitable_from_asyncio(call.func.as_ref(), checker.semantic())
        || is_awaitable_func(call.func.as_ref(), checker.semantic());

    // If call does not originate from asyncio or is not an async function, then it's not awaitable
    if !is_awaitable {
        return;
    }

    checker
        .report_diagnostic(MissingAwaitForCoroutine, call.range())
        .set_fix(Fix::unsafe_edit(Edit::range_replacement(
            checker.generator().expr(&generate_fix(call)),
            call.range(),
        )));
}

fn is_awaitable_from_asyncio(func: &Expr, semantic: &SemanticModel) -> bool {
    if let Some(call_path) = semantic.resolve_qualified_name(func) {
        return matches!(
            call_path.segments(),
            ["asyncio", "sleep" | "wait" | "wait_for" | "gather"]
        );
    }
    false
}

fn is_awaitable_func(func: &Expr, semantic: &SemanticModel) -> bool {
    let Expr::Name(ExprName { id, .. }) = func else {
        return false;
    };
    let Some(binding_id) = semantic.lookup_symbol(id) else {
        return false;
    };
    let binding = semantic.binding(binding_id);
    if let Some(node_id) = binding.source {
        let node = semantic.statement(node_id);
        if let Stmt::FunctionDef(StmtFunctionDef { is_async, name, .. }) = node {
            return *is_async && name.as_str() == id;
        }
    }
    false
}

fn possibly_missing_await(call: &ExprCall, semantic: &SemanticModel) -> bool {
    if let Stmt::Expr(StmtExpr { value, .. }) = semantic.current_statement() {
        if let Expr::Call(expr_call) = value.as_ref() {
            return expr_call == call;
        }
    }

    if let Some(Stmt::Assign(StmtAssign { value, .. })) = semantic.current_statement_parent() {
        if let Expr::Call(expr_call) = value.as_ref() {
            return expr_call == call;
        }
    }
    false
}

/// Generate a [`Fix`] to add `await` for coroutine.
///
/// For example:
/// - Given `asyncio.sleep(1)`, generate `await asyncio.sleep(1)`.
fn generate_fix(call: &ExprCall) -> Expr {
    Expr::Await(ExprAwait {
        node_index: AtomicNodeIndex::default(),
        value: Box::new(Expr::Call(call.clone())),
        range: TextRange::default(),
    })
}
