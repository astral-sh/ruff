use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::helpers::map_callable;
use ruff_python_ast::{self as ast, Expr, Stmt};
use ruff_python_semantic::ScopeKind;
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `yield` inside a context manager in an async generator.
///
/// ## Why is this bad?
/// Yielding inside a context manager in an async generator is unsafe because
/// the cleanup of the context manager may be delayed until the generator is
/// closed, at which point `await` is no longer allowed. This can lead to
/// resource leaks or other bugs.
///
/// For more information, see [PEP 533](https://peps.python.org/pep-0533/).
///
/// If the function is intended to yield only once and act as a context
/// manager, use `@asynccontextmanager`. If it's a true async generator
/// that yields multiple values, consumers should use `aclosing()` to
/// ensure timely cleanup, or the generator should be refactored to avoid
/// holding context managers open across yields.
///
/// ## Example
///
/// The following function yields once inside a context manager, but without
/// `@asynccontextmanager`, cleanup of the connection may be delayed:
/// ```python
/// async def open_connection():
///     async with connect() as conn:
///         yield conn
/// ```
///
/// If the function is intended to yield exactly once (i.e., it's a context
/// manager), add `@asynccontextmanager`:
/// ```python
/// from contextlib import asynccontextmanager
///
///
/// @asynccontextmanager
/// async def open_connection():
///     async with connect() as conn:
///         yield conn
/// ```
///
/// For async generators that yield multiple values, `@asynccontextmanager`
/// is not appropriate. Instead, refactor to avoid holding context managers
/// open across yields, or ensure consumers use `aclosing()` for timely
/// cleanup.
///
/// ## References
/// - [PEP 533 – Deterministic cleanup for iterators](https://peps.python.org/pep-0533/)
/// - [contextlib.aclosing](https://docs.python.org/3/library/contextlib.html#contextlib.aclosing)
/// - [trio.as_safe_channel](https://trio.readthedocs.io/en/latest/reference-core.html#trio.as_safe_channel)
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "NEXT_RUFF_VERSION")]
pub(crate) struct YieldInContextManagerInAsyncGenerator;

impl Violation for YieldInContextManagerInAsyncGenerator {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Yield in context manager in async generator may not trigger cleanup. Use `@asynccontextmanager` if appropriate, or refactor.".to_string()
    }
}

/// ASYNC119
pub(crate) fn yield_in_context_manager_in_async_generator(checker: &Checker, expr: &Expr) {
    // Check that the enclosing scope is an async function.
    let Some(function_def) = enclosing_async_function(checker) else {
        return;
    };

    // If the function is decorated with `@asynccontextmanager` or `@pytest.fixture`,
    // the yield is safe — these decorators handle async generator cleanup properly.
    if has_safe_decorator(checker, function_def) {
        return;
    }

    // Walk up the statement hierarchy to check if this yield is inside a `with` block.
    // Stop at function/class boundaries, since nested definitions create a new scope.
    for stmt in checker.semantic().current_statements() {
        match stmt {
            Stmt::With(_) => {
                checker.report_diagnostic(YieldInContextManagerInAsyncGenerator, expr.range());
                return;
            }
            Stmt::FunctionDef(_) | Stmt::ClassDef(_) => return,
            _ => {}
        }
    }
}

/// Returns the enclosing `async` function definition, if any.
fn enclosing_async_function<'a>(checker: &'a Checker) -> Option<&'a ast::StmtFunctionDef> {
    for scope in checker.semantic().current_scopes() {
        match scope.kind {
            ScopeKind::Function(function_def) if function_def.is_async => {
                return Some(function_def);
            }
            // Nested functions, lambdas, and classes break the chain.
            ScopeKind::Function(_) | ScopeKind::Lambda(_) | ScopeKind::Class(_) => return None,
            _ => {}
        }
    }
    None
}

/// Returns `true` if the function is decorated with `@asynccontextmanager`
/// or `@pytest.fixture`, which are known to handle async generator cleanup
/// safely.
fn has_safe_decorator(checker: &Checker, function_def: &ast::StmtFunctionDef) -> bool {
    function_def.decorator_list.iter().any(|decorator| {
        checker
            .semantic()
            .resolve_qualified_name(map_callable(&decorator.expression))
            .is_some_and(|qualified_name| {
                matches!(
                    qualified_name.segments(),
                    ["contextlib", "asynccontextmanager"]
                        | ["pytest" | "pytest_asyncio", "fixture"]
                )
            })
    })
}
