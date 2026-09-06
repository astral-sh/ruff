use bitflags::bitflags;
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::helpers::Truthiness;
use ruff_python_ast::identifier::except;
use ruff_python_ast::{self as ast, ExceptHandler, Expr, Stmt};
use ruff_python_semantic::SemanticModel;
use ruff_text_size::{Ranged, TextRange};

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;

/// ## What it does
/// Checks for exception handlers that may suppress cancellation exceptions.
///
/// ## Why is this bad?
/// Cancellation exceptions signal that an asynchronous task should stop. They
/// must generally be re-raised after any cleanup has completed. Suppressing a
/// cancellation exception may leave the task running despite a cancellation
/// request.
///
/// This rule detects handlers for `asyncio.CancelledError`, `trio.Cancelled`,
/// and `anyio.get_cancelled_exc_class()`. It also detects broad handlers for
/// `BaseException`, including bare `except` handlers.
///
/// It only checks whether every path raises an exception, not which exception
/// is raised.
///
/// ## Example
/// ```python
/// import asyncio
///
///
/// async def fn():
///     try:
///         await work()
///     except asyncio.CancelledError:
///         await cleanup()
/// ```
///
/// Use instead:
/// ```python
/// import asyncio
///
///
/// async def fn():
///     try:
///         await work()
///     except asyncio.CancelledError:
///         await cleanup()
///         raise
/// ```
///
/// If a broad handler intentionally suppresses other subclasses of
/// `BaseException`, catch and re-raise the cancellation exception first:
/// ```python
/// import asyncio
///
///
/// async def fn():
///     try:
///         await work()
///     except asyncio.CancelledError:
///         raise
///     except BaseException as exc:
///         handle_exception(exc)
/// ```
///
/// ## Known problems
/// A nested `try` with exception handlers is only considered to raise if its
/// `finally` suite always raises, or if its body, `else` suite, and every
/// handler always raise.
///
/// This rule does not account for exception-suppressing context managers. It
/// may also miss a non-raising path through an infinite loop when a raise
/// appears elsewhere in the handler.
///
/// On Python 3.7, `asyncio.CancelledError` was an `Exception` subclass, but
/// this rule does not treat `except Exception` as a cancellation handler.
///
/// ## References
/// - [Python documentation: `asyncio.CancelledError`](https://docs.python.org/3/library/asyncio-exceptions.html#asyncio.CancelledError)
/// - [AnyIO documentation: Finalization](https://anyio.readthedocs.io/en/stable/cancellation.html#finalization)
/// - [Trio documentation: Cancellation and timeouts](https://trio.readthedocs.io/en/stable/reference-core.html#cancellation-and-timeouts)
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "NEXT_RUFF_VERSION", category = Category::Suspicious)]
pub(crate) struct CancellationExceptionWithoutReraise {
    handler: CancellationHandlerKind,
}

impl Violation for CancellationExceptionWithoutReraise {
    #[derive_message_formats]
    fn message(&self) -> String {
        let Self { handler } = self;
        format!(
            "{} handler has a code path that does not raise an exception",
            handler.name()
        )
    }
}

#[derive(Clone, Copy)]
enum CancellationHandlerKind {
    BaseException,
    BareExcept,
    AsyncioCancelledError,
    AnyioCancelledError,
    TrioCancelled,
}

impl CancellationHandlerKind {
    const fn is_broad(self) -> bool {
        matches!(self, Self::BaseException | Self::BareExcept)
    }

    const fn name(self) -> &'static str {
        match self {
            Self::BaseException => "`BaseException`",
            Self::BareExcept => "Bare `except`",
            Self::AsyncioCancelledError => "`asyncio.CancelledError`",
            Self::AnyioCancelledError => "`anyio.get_cancelled_exc_class()`",
            Self::TrioCancelled => "`trio.Cancelled`",
        }
    }
}

/// ASYNC103
pub(crate) fn cancellation_exception_without_reraise(
    checker: &Checker,
    handlers: &[ExceptHandler],
) {
    let semantic = checker.semantic();
    let mut seen_recognized_handler = false;

    for handler in handlers {
        let ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler { type_, body, .. }) =
            handler;

        let matched = match type_.as_deref() {
            None => Some((
                CancellationHandlerKind::BareExcept,
                except(handler, checker.locator().contents()),
            )),
            Some(Expr::Tuple(ast::ExprTuple { elts, .. })) => elts
                .iter()
                .filter_map(|element| match_cancellation_exception(element, semantic))
                .min_by_key(|(kind, _)| kind.is_broad()),
            Some(type_) => match_cancellation_exception(type_, semantic),
        };
        let Some((kind, range)) = matched else {
            continue;
        };

        if kind.is_broad() && seen_recognized_handler {
            continue;
        }
        seen_recognized_handler = true;

        if analyze_body(body, semantic) != Flow::RAISE {
            checker.report_diagnostic(CancellationExceptionWithoutReraise { handler: kind }, range);
        }
    }
}

fn match_cancellation_exception(
    type_: &Expr,
    semantic: &SemanticModel,
) -> Option<(CancellationHandlerKind, TextRange)> {
    if semantic.match_builtin_expr(type_, "BaseException") {
        return Some((CancellationHandlerKind::BaseException, type_.range()));
    }

    if let Expr::Call(ast::ExprCall {
        func, arguments, ..
    }) = type_
    {
        if !arguments.is_empty() {
            return None;
        }
        let qualified_name = semantic.resolve_qualified_name(func)?;
        return matches!(
            qualified_name.segments(),
            ["anyio", "get_cancelled_exc_class"]
        )
        .then_some((CancellationHandlerKind::AnyioCancelledError, type_.range()));
    }

    let qualified_name = semantic.resolve_qualified_name(type_)?;
    let kind = match qualified_name.segments() {
        ["asyncio", "CancelledError"] | ["asyncio", "exceptions", "CancelledError"] => {
            CancellationHandlerKind::AsyncioCancelledError
        }
        ["trio", "Cancelled"] => CancellationHandlerKind::TrioCancelled,
        _ => return None,
    };
    Some((kind, type_.range()))
}

bitflags! {
    /// The possible outcomes of executing a statement or body.
    #[derive(Clone, Copy, Eq, PartialEq)]
    struct Flow: u8 {
        const FALLTHROUGH = 1 << 0;
        const RAISE = 1 << 1;
        const RETURN = 1 << 2;
        const BREAK = 1 << 3;
        const CONTINUE = 1 << 4;
        const NEVER_EXITS = 1 << 5;
    }
}

impl Flow {
    /// Sequences `next` after `self` on paths that can fall through.
    fn and_then(mut self, next: Self) -> Self {
        if self.contains(Self::FALLTHROUGH) {
            self.remove(Self::FALLTHROUGH);
            self | next
        } else {
            self
        }
    }
}

#[derive(Clone, Copy)]
enum LoopExecution {
    MayBeEmpty,
    AtLeastOnce,
    Infinite,
}

fn analyze_body(body: &[Stmt], semantic: &SemanticModel) -> Flow {
    body.iter().fold(Flow::FALLTHROUGH, |flow, statement| {
        flow.and_then(analyze_statement(statement, semantic))
    })
}

fn analyze_statement(statement: &Stmt, semantic: &SemanticModel) -> Flow {
    match statement {
        Stmt::Raise(_) => Flow::RAISE,
        Stmt::Return(_) => Flow::RETURN,
        Stmt::Break(_) => Flow::BREAK,
        Stmt::Continue(_) => Flow::CONTINUE,
        Stmt::If(ast::StmtIf {
            body,
            elif_else_clauses,
            ..
        }) => {
            let mut flow = analyze_body(body, semantic);
            let mut exhaustive = false;
            for clause in elif_else_clauses {
                exhaustive |= clause.test.is_none();
                flow |= analyze_body(&clause.body, semantic);
            }
            if !exhaustive {
                flow |= Flow::FALLTHROUGH;
            }
            flow
        }
        Stmt::Match(ast::StmtMatch { cases, .. }) => {
            let mut flow = Flow::empty();
            let mut exhaustive = false;
            for case in cases {
                exhaustive |= case.guard.is_none() && case.pattern.is_irrefutable();
                flow |= analyze_body(&case.body, semantic);
            }
            if !exhaustive {
                flow |= Flow::FALLTHROUGH;
            }
            flow
        }
        Stmt::Try(ast::StmtTry {
            body,
            handlers,
            orelse,
            finalbody,
            ..
        }) => {
            let inner_flow = if handlers.is_empty() {
                analyze_body(body, semantic)
            } else {
                // Any statement in the body can raise and be swallowed by a
                // handler, so raises only count when every handler raises.
                let escapes = Flow::BREAK | Flow::CONTINUE | Flow::RETURN;
                let body_flow = analyze_body(body, semantic);
                let orelse_flow = analyze_body(orelse, semantic);
                let mut escaping = body_flow | orelse_flow;
                let mut all_handlers_raise = true;
                for handler in handlers {
                    let ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler {
                        body: handler_body,
                        ..
                    }) = handler;
                    let handler_flow = analyze_body(handler_body, semantic);
                    all_handlers_raise &= handler_flow == Flow::RAISE;
                    escaping |= handler_flow;
                }

                if all_handlers_raise && body_flow.and_then(orelse_flow) == Flow::RAISE {
                    Flow::RAISE
                } else {
                    Flow::FALLTHROUGH | (escaping & escapes)
                }
            };

            // `finally` runs last, so its exits take precedence over the inner
            // outcome. A never-exiting body never reaches `finally`.
            let never_exits = inner_flow & Flow::NEVER_EXITS;
            analyze_body(finalbody, semantic).and_then(inner_flow) | never_exits
        }
        Stmt::For(ast::StmtFor {
            iter, body, orelse, ..
        }) => {
            let execution = if iterable_guaranteed_non_empty(iter, semantic) {
                LoopExecution::AtLeastOnce
            } else {
                LoopExecution::MayBeEmpty
            };
            analyze_loop(body, orelse, execution, semantic)
        }
        Stmt::While(ast::StmtWhile {
            test, body, orelse, ..
        }) => {
            let is_infinite = Truthiness::from_expr(test, |id| semantic.has_builtin_binding(id))
                .into_bool()
                == Some(true);
            let execution = if is_infinite {
                LoopExecution::Infinite
            } else {
                LoopExecution::MayBeEmpty
            };
            analyze_loop(body, orelse, execution, semantic)
        }
        Stmt::With(ast::StmtWith { body, .. }) => analyze_body(body, semantic),
        // A class body executes as part of the `class` statement.
        Stmt::ClassDef(ast::StmtClassDef { body, .. }) => analyze_body(body, semantic),
        _ => Flow::FALLTHROUGH,
    }
}

fn analyze_loop(
    body: &[Stmt],
    orelse: &[Stmt],
    execution: LoopExecution,
    semantic: &SemanticModel,
) -> Flow {
    let body_flow = analyze_body(body, semantic);

    let mut flow = body_flow & (Flow::RAISE | Flow::RETURN | Flow::NEVER_EXITS);
    if body_flow.contains(Flow::BREAK) {
        flow |= Flow::FALLTHROUGH;
    }

    if matches!(execution, LoopExecution::Infinite)
        && (body_flow.contains(Flow::FALLTHROUGH) && !body_flow.contains(Flow::BREAK)
            || body_flow == Flow::CONTINUE)
    {
        flow |= Flow::NEVER_EXITS;
    }

    let reaches_orelse = match execution {
        LoopExecution::Infinite => false,
        LoopExecution::MayBeEmpty => true,
        LoopExecution::AtLeastOnce => body_flow.intersects(Flow::FALLTHROUGH | Flow::CONTINUE),
    };
    if reaches_orelse {
        flow |= analyze_body(orelse, semantic);
    }

    flow
}

fn iterable_guaranteed_non_empty(iterable: &Expr, semantic: &SemanticModel) -> bool {
    match iterable {
        Expr::List(ast::ExprList { elts, .. })
        | Expr::Set(ast::ExprSet { elts, .. })
        | Expr::Tuple(ast::ExprTuple { elts, .. }) => elts.iter().any(|element| match element {
            Expr::Starred(ast::ExprStarred { value, .. }) => {
                iterable_guaranteed_non_empty(value, semantic)
            }
            _ => true,
        }),
        Expr::Dict(ast::ExprDict { items, .. }) => items.iter().any(|item| {
            item.key.is_some()
                || (item.value.is_dict_expr()
                    && iterable_guaranteed_non_empty(&item.value, semantic))
        }),
        Expr::StringLiteral(ast::ExprStringLiteral { value, .. }) => !value.is_empty(),
        Expr::BytesLiteral(ast::ExprBytesLiteral { value, .. }) => !value.is_empty(),
        Expr::Call(ast::ExprCall {
            func, arguments, ..
        }) if arguments.keywords.is_empty() => {
            semantic.match_builtin_expr(func, "range")
                && range_guaranteed_non_empty(&arguments.args)
        }
        _ => false,
    }
}

fn range_guaranteed_non_empty(arguments: &[Expr]) -> bool {
    let (start, stop, step) = match arguments {
        [stop] => (Some(0), integer_value(stop), Some(1)),
        [start, stop] => (integer_value(start), integer_value(stop), Some(1)),
        [start, stop, step] => (
            integer_value(start),
            integer_value(stop),
            integer_value(step),
        ),
        _ => return false,
    };
    let (Some(start), Some(stop), Some(step)) = (start, stop, step) else {
        return false;
    };
    match start.cmp(&stop) {
        std::cmp::Ordering::Less => step > 0,
        std::cmp::Ordering::Greater => step < 0,
        std::cmp::Ordering::Equal => false,
    }
}

/// The integer value of an expression such as `3`, `-1`, or `True`.
fn integer_value(expr: &Expr) -> Option<i64> {
    fn int_literal(expr: &Expr) -> Option<i64> {
        let Expr::NumberLiteral(ast::ExprNumberLiteral {
            value: ast::Number::Int(value),
            ..
        }) = expr
        else {
            return None;
        };
        // Int literals are non-negative, so an overflowing one exceeds `i64::MAX`.
        Some(value.as_i64().unwrap_or(i64::MAX))
    }

    match expr {
        Expr::BooleanLiteral(ast::ExprBooleanLiteral { value, .. }) => Some(i64::from(*value)),
        Expr::UnaryOp(ast::ExprUnaryOp { op, operand, .. }) => match op {
            ast::UnaryOp::UAdd => int_literal(operand),
            ast::UnaryOp::USub => int_literal(operand)?.checked_neg(),
            ast::UnaryOp::Invert | ast::UnaryOp::Not => None,
        },
        _ => int_literal(expr),
    }
}
