use bitflags::bitflags;

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
/// The rule visits branches and loops in source order without analyzing
/// execution paths or rebinding of scope variables. It can therefore miss
/// unshielded cancellation points or report ones that are shielded at runtime.
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
    if !function.is_async
        || !checker
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

// Keep the cancellation families separate: catching Trio cancellation does
// not consume an asyncio cancellation raised by AnyIO's asyncio backend.
bitflags! {
    #[derive(Clone, Copy)]
    struct CancellationTypes: u8 {
        const TRIO = 1 << 0;
        const ASYNCIO = 1 << 1;
        // A catch-all handler does not establish that asyncio cancellation is
        // relevant. Keep it separate from the explicit cancellation families
        // until deciding which remaining families a handler can catch.
        const CATCH_ALL = 1 << 2;
    }
}

fn enables_shield(expr: &Expr) -> bool {
    expr.is_literal_expr()
        && matches!(
            Truthiness::from_expr(expr, |_| false),
            Truthiness::True | Truthiness::Truthy
        )
}

impl<'a> CleanupVisitor<'a, '_> {
    fn checkpoint(&self, range: TextRange) -> bool {
        if let Some(context) = self.context
            && !self.scopes[self.boundary..]
                .iter()
                .any(|scope| scope.shielded)
        {
            self.checker
                .report_diagnostic(AwaitInFinallyOrCancelled { context }, range);
            return true;
        }
        false
    }

    fn cancellation_types(&self, expr: &Expr) -> CancellationTypes {
        match expr {
            Expr::Tuple(tuple) => tuple
                .iter()
                .fold(CancellationTypes::empty(), |types, expr| {
                    types | self.cancellation_types(expr)
                }),
            Expr::Call(call) => {
                if call.arguments.is_empty()
                    && self
                        .checker
                        .semantic()
                        .resolve_qualified_name(&call.func)
                        .is_some_and(|name| name.segments() == ["anyio", "get_cancelled_exc_class"])
                {
                    CancellationTypes::CATCH_ALL
                } else {
                    CancellationTypes::empty()
                }
            }
            _ => self.checker.semantic().resolve_qualified_name(expr).map_or(
                CancellationTypes::empty(),
                |name| match name.segments() {
                    ["", "BaseException"] => CancellationTypes::CATCH_ALL,
                    ["trio", "Cancelled"] => CancellationTypes::TRIO,
                    ["asyncio", "CancelledError"] | ["asyncio", "exceptions", "CancelledError"] => {
                        CancellationTypes::ASYNCIO
                    }
                    _ => CancellationTypes::empty(),
                },
            ),
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
        let handler_types: Vec<_> = stmt
            .handlers
            .iter()
            .map(|handler| {
                let ast::ExceptHandler::ExceptHandler(handler) = handler;
                handler
                    .type_
                    .as_ref()
                    .map_or(CancellationTypes::CATCH_ALL, |expr| {
                        self.cancellation_types(expr)
                    })
            })
            .collect();
        let mut remaining = CancellationTypes::TRIO;
        if self.checker.semantic().seen_module(Modules::ANYIO)
            || handler_types
                .iter()
                .any(|types| types.contains(CancellationTypes::ASYNCIO))
        {
            remaining.insert(CancellationTypes::ASYNCIO);
        }
        for (handler, types) in stmt.handlers.iter().zip(handler_types) {
            let ast::ExceptHandler::ExceptHandler(handler) = handler;
            self.context = context;
            self.boundary = boundary;
            let types = if types.contains(CancellationTypes::CATCH_ALL) {
                CancellationTypes::TRIO | CancellationTypes::ASYNCIO
            } else {
                types
            };
            if context.is_none() {
                self.boundary = self.scopes.len();
                if types.intersects(remaining) {
                    self.context = Some(CleanupContext::Except);
                    if !stmt.is_star {
                        remaining.remove(types);
                    }
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
                // Defaults and decorators execute in the enclosing scope.
                for decorator in &function.decorator_list {
                    self.visit_decorator(decorator);
                }
                self.visit_parameters(&function.parameters);
                if let Some(returns) = &function.returns {
                    self.visit_annotation(returns);
                }
            }
            Stmt::ClassDef(class) => {
                for decorator in &class.decorator_list {
                    self.visit_decorator(decorator);
                }
                if let Some(arguments) = &class.arguments {
                    self.visit_arguments(arguments);
                }
                self.visit_body(&class.body);
            }
            Stmt::Try(stmt) => self.visit_try(stmt),
            Stmt::With(stmt) => {
                let count = self.scopes.len();
                let mut exits = Vec::new();
                let mut tracked = false;
                for item in &stmt.items {
                    self.visit_expr(&item.context_expr);
                    let scope = self.scope(item);
                    let checkpoint =
                        stmt.is_async && !scope.as_ref().is_some_and(|scope| scope.task_group);
                    let reported = checkpoint && self.checkpoint(item.range());
                    exits.push((item, self.scopes.len(), checkpoint, reported));
                    if !tracked && let Some(scope) = scope {
                        self.scopes.push(scope);
                        tracked = true;
                    }
                    if let Some(target) = &item.optional_vars {
                        self.visit_expr(target);
                    }
                }
                self.visit_body(&stmt.body);
                for (item, scopes, checkpoint, reported) in exits.into_iter().rev() {
                    self.scopes.truncate(scopes);
                    if checkpoint && !reported {
                        self.checkpoint(item.range());
                    }
                }
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
            Stmt::AnnAssign(assign) => {
                if let Some(value) = &assign.value {
                    self.visit_expr(value);
                }
                self.visit_expr(&assign.target);
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
            // A generator expression's body is deferred until iteration.
            Expr::Generator(generator) => {
                if let Some(first) = generator.generators.first() {
                    self.visit_expr(&first.iter);
                }
            }
            Expr::Lambda(lambda) => {
                if let Some(parameters) = &lambda.parameters {
                    self.visit_parameters(parameters);
                }
            }
            _ => visitor::walk_expr(self, expr),
        }
    }

    fn visit_comprehension(&mut self, comprehension: &'a ast::Comprehension) {
        if comprehension.is_async {
            self.checkpoint(comprehension.range());
        }
        visitor::walk_comprehension(self, comprehension);
    }

    fn visit_annotation(&mut self, expr: &'a Expr) {
        if !self.checker.semantic().future_annotations_or_stub()
            && self.checker.target_version() < ast::PythonVersion::PY314
        {
            self.visit_expr(expr);
        }
    }
}
