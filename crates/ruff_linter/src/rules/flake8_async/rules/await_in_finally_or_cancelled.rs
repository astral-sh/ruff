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
/// Shield assignments across loops and exception paths are treated
/// conservatively, which can report code whose shielding depends on control flow.
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

#[derive(Clone)]
struct CancelScope<'a> {
    name: Option<&'a str>,
    task_group: bool,
    shielded: bool,
}

struct CleanupVisitor<'a, 'b> {
    checker: &'b Checker<'a>,
    context: Option<CleanupContext>,
    scopes: Vec<CancelScope<'a>>,
    /// Scopes entered before the current cleanup cannot shield its checkpoints.
    boundary: usize,
}

// Keep the cancellation families separate: catching Trio cancellation does
// not consume an asyncio cancellation raised by AnyIO's asyncio backend.
const TRIO_CANCELLED: u8 = 1;
const ASYNCIO_CANCELLED: u8 = 2;
const ALL_CANCELLED: u8 = 4;

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

    fn cancellation_types(&self, expr: &Expr) -> u8 {
        if let Expr::Tuple(tuple) = expr {
            return tuple
                .iter()
                .fold(0, |types, expr| types | self.cancellation_types(expr));
        }
        if let Expr::Call(call) = expr {
            return if call.arguments.is_empty()
                && self
                    .checker
                    .semantic()
                    .resolve_qualified_name(&call.func)
                    .is_some_and(|name| name.segments() == ["anyio", "get_cancelled_exc_class"])
            {
                ALL_CANCELLED
            } else {
                0
            };
        }
        self.checker
            .semantic()
            .resolve_qualified_name(expr)
            .map_or(0, |name| match name.segments() {
                ["", "BaseException"] => ALL_CANCELLED,
                ["trio", "Cancelled"] => TRIO_CANCELLED,
                ["asyncio", "CancelledError"] | ["asyncio", "exceptions", "CancelledError"] => {
                    ASYNCIO_CANCELLED
                }
                _ => 0,
            })
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

    fn assign(&mut self, target: &Expr, value: Option<&Expr>) {
        match target {
            Expr::Tuple(tuple) => {
                for target in &tuple.elts {
                    self.assign(target, None);
                }
                return;
            }
            Expr::List(list) => {
                for target in &list.elts {
                    self.assign(target, None);
                }
                return;
            }
            Expr::Starred(starred) => {
                self.assign(&starred.value, None);
                return;
            }
            _ => {}
        }
        // Rebinding the handle does not change the original scope's shield,
        // but subsequent attribute writes must no longer modify that scope.
        if let Expr::Name(name) = target {
            for scope in &mut self.scopes {
                if scope.name == Some(name.id.as_str()) {
                    scope.name = None;
                }
            }
            return;
        }
        let Expr::Attribute(attribute) = target else {
            return;
        };
        if attribute.attr.as_str() != "shield" {
            return;
        }
        let (receiver, task_group) = match &*attribute.value {
            Expr::Attribute(attribute) if attribute.attr.as_str() == "cancel_scope" => {
                (&*attribute.value, true)
            }
            expr => (expr, false),
        };
        if let Expr::Name(name) = receiver
            && let Some(scope) = self.scopes.iter_mut().rev().find(|scope| {
                scope.name == Some(name.id.as_str()) && scope.task_group == task_group
            })
        {
            scope.shielded = value.is_some_and(enables_shield);
        }
    }

    fn merge_scopes(&mut self, other: &[CancelScope<'a>]) {
        for (scope, other) in self.scopes.iter_mut().zip(other) {
            scope.shielded &= other.shielded;
            if scope.name != other.name {
                scope.name = None;
            }
        }
    }

    fn visit_try(&mut self, stmt: &'a ast::StmtTry) {
        let context = self.context;
        let boundary = self.boundary;
        let before = self.scopes.clone();
        self.invalidate_assignments(&stmt.body);
        let handler_entry = self.scopes.clone();
        self.scopes = before;
        self.visit_body(&stmt.body);
        self.visit_body(&stmt.orelse);
        let mut outcomes = self.scopes.clone();
        let handler_types: Vec<_> = stmt
            .handlers
            .iter()
            .map(|handler| {
                let ast::ExceptHandler::ExceptHandler(handler) = handler;
                handler
                    .type_
                    .as_ref()
                    .map_or(ALL_CANCELLED, |expr| self.cancellation_types(expr))
            })
            .collect();
        let mut remaining = TRIO_CANCELLED;
        if self.checker.semantic().seen_module(Modules::ANYIO)
            || handler_types
                .iter()
                .any(|types| types & ASYNCIO_CANCELLED != 0)
        {
            remaining |= ASYNCIO_CANCELLED;
        }
        for (handler, types) in stmt.handlers.iter().zip(handler_types) {
            let ast::ExceptHandler::ExceptHandler(handler) = handler;
            self.context = context;
            self.boundary = boundary;
            self.scopes.clone_from(&handler_entry);
            if let Some(type_) = &handler.type_ {
                self.visit_expr(type_);
            }
            let types = if types & ALL_CANCELLED != 0 {
                TRIO_CANCELLED | ASYNCIO_CANCELLED
            } else {
                types
            };
            if context.is_none() && types & remaining != 0 {
                self.context = Some(CleanupContext::Except);
                self.boundary = self.scopes.len();
                if !stmt.is_star {
                    remaining &= !types;
                }
            }
            self.visit_body(&handler.body);
            self.merge_scopes(&outcomes);
            outcomes.clone_from(&self.scopes);
        }
        self.scopes = outcomes;
        // The finally block also runs on exceptions not caught by any handler.
        self.merge_scopes(&handler_entry);
        self.context = Some(CleanupContext::Finally);
        self.boundary = self.scopes.len();
        self.visit_body(&stmt.finalbody);
        self.context = context;
        self.boundary = boundary;
    }

    fn invalidate_assignments(&mut self, body: &'a [Stmt]) {
        // A loop can reach its next iteration after any write in the body, and
        // an exception can transfer control before or after a write. Start
        // these paths without assuming that a modified shield remains enabled.
        AssignmentVisitor { visitor: self }.visit_body(body);
    }
}

struct AssignmentVisitor<'a, 'b, 'c> {
    visitor: &'c mut CleanupVisitor<'a, 'b>,
}

impl<'a> Visitor<'a> for AssignmentVisitor<'a, '_, '_> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::FunctionDef(_) | Stmt::ClassDef(_) => return,
            Stmt::Assign(stmt) => {
                for target in &stmt.targets {
                    self.visitor.assign(target, None);
                }
            }
            Stmt::AnnAssign(stmt) if stmt.value.is_some() => {
                self.visitor.assign(&stmt.target, None);
            }
            Stmt::AugAssign(stmt) => self.visitor.assign(&stmt.target, None),
            _ => {}
        }
        visitor::walk_stmt(self, stmt);
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
            }
            Stmt::Try(stmt) => self.visit_try(stmt),
            Stmt::With(stmt) => {
                let count = self.scopes.len();
                let mut exits = Vec::new();
                for item in &stmt.items {
                    self.visit_expr(&item.context_expr);
                    let scope = self.scope(item);
                    let checkpoint =
                        stmt.is_async && !scope.as_ref().is_some_and(|scope| scope.task_group);
                    let reported = checkpoint && self.checkpoint(item.range());
                    exits.push((item, self.scopes.len(), checkpoint, reported));
                    if let Some(target) = &item.optional_vars {
                        self.assign(target, None);
                    }
                    if let Some(scope) = scope {
                        self.scopes.push(scope);
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
                for target in &assign.targets {
                    self.assign(target, Some(&assign.value));
                }
            }
            Stmt::AnnAssign(stmt) => {
                if let Some(value) = &stmt.value {
                    self.visit_expr(value);
                }
                self.visit_expr(&stmt.target);
                if let Some(value) = &stmt.value {
                    self.assign(&stmt.target, Some(value));
                }
            }
            Stmt::AugAssign(assign) => {
                visitor::walk_stmt(self, stmt);
                self.assign(&assign.target, None);
            }
            Stmt::If(stmt) => {
                self.visit_expr(&stmt.test);
                let before = self.scopes.clone();
                self.visit_body(&stmt.body);
                let mut outcomes = self.scopes.clone();
                let mut has_else = false;
                for clause in &stmt.elif_else_clauses {
                    self.scopes.clone_from(&before);
                    self.visit_elif_else_clause(clause);
                    has_else |= clause.test.is_none();
                    self.merge_scopes(&outcomes);
                    outcomes.clone_from(&self.scopes);
                }
                self.scopes = outcomes;
                if !has_else {
                    self.merge_scopes(&before);
                }
            }
            Stmt::For(stmt) => {
                self.visit_expr(&stmt.iter);
                self.invalidate_assignments(&stmt.body);
                if stmt.is_async {
                    self.checkpoint(stmt.range());
                }
                self.visit_expr(&stmt.target);
                self.assign(&stmt.target, None);
                let before = self.scopes.clone();
                self.visit_body(&stmt.body);
                self.merge_scopes(&before);
                self.visit_body(&stmt.orelse);
            }
            Stmt::While(stmt) => {
                self.invalidate_assignments(&stmt.body);
                self.visit_expr(&stmt.test);
                let before = self.scopes.clone();
                self.visit_body(&stmt.body);
                self.merge_scopes(&before);
                self.visit_body(&stmt.orelse);
            }
            Stmt::Match(stmt) => {
                self.visit_expr(&stmt.subject);
                let before = self.scopes.clone();
                let mut outcomes = before.clone();
                for case in &stmt.cases {
                    self.scopes.clone_from(&before);
                    self.visit_match_case(case);
                    self.merge_scopes(&outcomes);
                    outcomes.clone_from(&self.scopes);
                }
                self.scopes = outcomes;
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
            Expr::Named(named) => {
                self.visit_expr(&named.value);
                self.assign(&named.target, None);
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
