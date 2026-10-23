use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::helpers::Truthiness;
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{self as ast, Expr, Stmt};
use ruff_python_semantic::Modules;
use ruff_text_size::{Ranged, TextRange};

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::rules::flake8_async::helpers::{AsyncModule, MethodName};

/// ## What it does
/// Checks for unshielded cancellation points in `finally` blocks, exception
/// handlers that can catch cancellation, and asynchronous `__aexit__` methods.
/// This includes `await`, `async for`, and `async with`.
///
/// This rule applies to modules that import `trio` or `anyio`. It does not apply to
/// asyncio-only code, which uses different cancellation semantics.
///
/// ## Why is this bad?
/// Trio and `anyio` use level cancellation: once a cancel scope is cancelled,
/// subsequent cancellation points raise again. Awaiting during cleanup can
/// therefore interrupt the cleanup and leave resources open.
///
/// Enter a shielded cancel scope inside the cleanup block. A shield outside the
/// block is insufficient, since cancellation can originate inside that shield.
/// A timeout can bound the cleanup duration, but is not required by this rule.
///
/// Calls to `.aclose()` without arguments, `trio.aclose_forcefully()` and
/// `anyio.aclose_forcefully()`, and the argument-free
/// `trio.lowlevel.cancel_shielded_checkpoint()` and its `anyio` equivalent are exempt.
/// Entering and exiting a Trio nursery or `anyio` task group is also exempt;
/// cancellation points in their bodies still require shielding.
///
/// ## Example
/// ```python
/// import anyio
///
///
/// async def use_session():
///     session = await login()
///     try:
///         await work(session)
///     finally:
///         await session.close()
/// ```
///
/// Use instead:
/// ```python
/// import anyio
///
///
/// async def use_session():
///     session = await login()
///     try:
///         await work(session)
///     finally:
///         with anyio.CancelScope(shield=True):
///             await session.close()
/// ```
///
/// ## Known problems
/// This rule recognizes literal shielding values and assignments to the
/// `shield` attribute of a locally named cancel scope (or a nursery's or task
/// group's `cancel_scope`). It cannot infer shielding performed by helper
/// functions, aliases of cancel scope objects, or dynamically computed values.
/// Only single-target assignments with literal values update the tracked shield
/// state. Only the first recognized cancel scope in a `with` statement is tracked.
/// Async context managers are checked before their bodies, so changes to shielding
/// before exit are not modeled. Only the first cancellation-catching handler of
/// a try statement establishes a cleanup context, including for exception groups.
/// The rule visits branches and loops in source order without analyzing
/// execution paths or rebinding of scope variables. It can therefore miss
/// unshielded cancellation points or report ones that are shielded at runtime.
/// Deferred generator and lambda bodies are visited even though creating those
/// expressions does not execute their bodies.
/// It assumes that argument-free `.aclose()` methods implement cancellation-safe
/// cleanup, without checking the receiver's type.
///
/// ## References
/// - [AnyIO cancellation and shielding](https://anyio.readthedocs.io/en/stable/cancellation.html#shielding)
/// - [Trio cancellation and timeouts](https://trio.readthedocs.io/en/stable/reference-core.html#cancellation-and-timeouts)
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "NEXT_RUFF_VERSION", category = Category::Suspicious)]
pub(crate) struct AwaitInFinallyOrCancelled {
    context: CleanupContext,
}

impl Violation for AwaitInFinallyOrCancelled {
    #[derive_message_formats]
    fn message(&self) -> String {
        let context = self.context;
        format!("Cancellation point in {context} must be protected by a shielded cancel scope")
    }
}

#[derive(Clone, Copy)]
enum CleanupContext {
    Finally,
    Except,
    AsyncExit,
}

impl std::fmt::Display for CleanupContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Finally => "`finally`",
            Self::Except => "a cancellation-catching exception handler",
            Self::AsyncExit => "`__aexit__`",
        })
    }
}

/// ASYNC102
pub(crate) fn await_in_finally_or_cancelled<'a>(
    checker: &Checker<'a>,
    function: &'a ast::StmtFunctionDef,
) {
    if !checker
        .semantic()
        .seen_module(Modules::TRIO | Modules::ANYIO)
    {
        return;
    }

    // Run after name resolution so imports and shadowed names in the function
    // body are available. Each function is visited once, independently of its
    // enclosing cleanup context.
    CleanupVisitor {
        checker,
        context: (function.name.as_str() == "__aexit__").then_some(CleanupContext::AsyncExit),
        scopes: Vec::new(),
        boundary: 0,
    }
    .visit_body(&function.body);
}

struct CancelScope<'a> {
    name: Option<&'a str>,
    task_group: bool,
    shielded: bool,
}

struct CleanupVisitor<'a, 'b> {
    checker: &'b Checker<'a>,
    context: Option<CleanupContext>,
    scopes: Vec<CancelScope<'a>>,
    /// Scopes entered before the current cleanup are excluded from shield checks
    /// and assignments.
    boundary: usize,
}

fn enables_shield(expr: &Expr) -> bool {
    expr.is_literal_expr()
        && matches!(
            Truthiness::from_expr(expr, |_| false),
            Truthiness::True | Truthiness::Truthy
        )
}

impl<'a> CleanupVisitor<'a, '_> {
    fn checkpoint(&self, range: TextRange) {
        if let Some(context) = self.context
            && !self.scopes[self.boundary..]
                .iter()
                .any(|scope| scope.shielded)
        {
            self.checker
                .report_diagnostic(AwaitInFinallyOrCancelled { context }, range);
        }
    }

    fn catches_cancellation(&self, expr: &Expr) -> bool {
        match expr {
            Expr::Tuple(tuple) => tuple.iter().any(|expr| self.catches_cancellation(expr)),
            Expr::Call(call) => {
                call.arguments.is_empty()
                    && self
                        .checker
                        .semantic()
                        .resolve_qualified_name(&call.func)
                        .is_some_and(|name| name.segments() == ["anyio", "get_cancelled_exc_class"])
            }
            _ => self
                .checker
                .semantic()
                .resolve_qualified_name(expr)
                .is_some_and(|name| {
                    matches!(
                        name.segments(),
                        ["", "BaseException"]
                            | ["trio", "Cancelled"]
                            | ["asyncio", "CancelledError"]
                            | ["asyncio", "exceptions", "CancelledError"]
                    )
                }),
        }
    }

    fn safe_await(&self, expr: &Expr) -> bool {
        let Expr::Call(call) = expr else {
            return false;
        };
        if call.arguments.is_empty()
            && let Expr::Attribute(attribute) = &*call.func
            && attribute.attr.as_str() == "aclose"
        {
            return true;
        }
        self.checker
            .semantic()
            .resolve_qualified_name(&call.func)
            .is_some_and(|name| match name.segments() {
                ["trio" | "anyio", "aclose_forcefully"] => true,
                ["trio" | "anyio", "lowlevel", "cancel_shielded_checkpoint"] => {
                    call.arguments.is_empty()
                }
                _ => false,
            })
    }

    fn scope(&self, item: &'a ast::WithItem) -> Option<CancelScope<'a>> {
        let call = item.context_expr.as_call_expr()?;
        let name = self.checker.semantic().resolve_qualified_name(&call.func)?;
        let task_group = matches!(
            name.segments(),
            ["trio", "open_nursery"] | ["anyio", "create_task_group"]
        );
        if !(task_group
            || AsyncModule::try_from(&name) != Some(AsyncModule::AsyncIo)
                && MethodName::try_from(&name).is_some_and(MethodName::is_timeout_context))
        {
            return None;
        }
        Some(CancelScope {
            name: item
                .optional_vars
                .as_ref()
                .and_then(|expr| expr.as_name_expr().map(|name| name.id.as_str())),
            task_group,
            shielded: !task_group
                && call
                    .arguments
                    .find_keyword("shield")
                    .is_some_and(|kw| enables_shield(&kw.value)),
        })
    }

    fn visit_try(&mut self, stmt: &'a ast::StmtTry) {
        let context = self.context;
        let boundary = self.boundary;
        self.visit_body(&stmt.body);
        let mut cancelled_caught = false;
        for handler in &stmt.handlers {
            let ast::ExceptHandler::ExceptHandler(handler) = handler;
            self.context = context;
            self.boundary = boundary;
            if context.is_none() {
                self.boundary = self.scopes.len();
                if !cancelled_caught
                    && handler
                        .type_
                        .as_ref()
                        .is_none_or(|expr| self.catches_cancellation(expr))
                {
                    self.context = Some(CleanupContext::Except);
                    cancelled_caught = true;
                }
            }
            if let Some(type_) = &handler.type_ {
                self.visit_expr(type_);
            }
            self.visit_body(&handler.body);
        }
        self.context = context;
        self.boundary = boundary;
        self.visit_body(&stmt.orelse);
        self.context = Some(CleanupContext::Finally);
        self.boundary = self.scopes.len();
        self.visit_body(&stmt.finalbody);
        self.context = context;
        self.boundary = boundary;
    }
}

impl<'a> Visitor<'a> for CleanupVisitor<'a, '_> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::FunctionDef(function) => {
                // Match flake8-async's independent context for definition headers.
                // Function bodies are checked separately after name resolution.
                let context = self.context;
                let boundary = self.boundary;
                self.context =
                    (function.name.as_str() == "__aexit__").then_some(CleanupContext::AsyncExit);
                self.boundary = self.scopes.len();
                for decorator in &function.decorator_list {
                    self.visit_decorator(decorator);
                }
                if let Some(type_params) = &function.type_params {
                    self.visit_type_params(type_params);
                }
                self.visit_parameters(&function.parameters);
                if let Some(returns) = &function.returns {
                    self.visit_annotation(returns);
                }
                self.context = context;
                self.boundary = boundary;
            }
            Stmt::Try(stmt) => self.visit_try(stmt),
            Stmt::With(with_stmt) => {
                if with_stmt.is_async
                    && with_stmt
                        .items
                        .iter()
                        .any(|item| !self.scope(item).is_some_and(|scope| scope.task_group))
                {
                    self.checkpoint(with_stmt.range());
                }
                let count = self.scopes.len();
                if let Some(scope) = with_stmt.items.iter().find_map(|item| self.scope(item)) {
                    self.scopes.push(scope);
                }
                visitor::walk_stmt(self, stmt);
                self.scopes.truncate(count);
            }
            Stmt::Assign(assign) => {
                visitor::walk_stmt(self, stmt);
                if let [Expr::Attribute(target)] = assign.targets.as_slice()
                    && target.attr.as_str() == "shield"
                    && assign.value.is_literal_expr()
                {
                    let receiver = match &*target.value {
                        Expr::Attribute(attribute) if attribute.attr.as_str() == "cancel_scope" => {
                            &*attribute.value
                        }
                        expr => expr,
                    };
                    if let Expr::Name(name) = receiver {
                        for scope in &mut self.scopes[self.boundary..] {
                            if scope.name == Some(name.id.as_str()) {
                                scope.shielded = enables_shield(&assign.value);
                            }
                        }
                    }
                }
            }
            Stmt::For(for_stmt) if for_stmt.is_async => {
                self.checkpoint(for_stmt.range());
                visitor::walk_stmt(self, stmt);
            }
            _ => visitor::walk_stmt(self, stmt),
        }
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr {
            Expr::Await(await_) => {
                self.visit_expr(&await_.value);
                if !self.safe_await(&await_.value) {
                    self.checkpoint(expr.range());
                }
            }
            _ => visitor::walk_expr(self, expr),
        }
    }
}
