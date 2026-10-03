use itertools::Itertools;
use ruff_diagnostics::Applicability;
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::name::UnqualifiedName;
use ruff_python_ast::{self as ast, ExceptHandler, Expr, ExprContext};
use ruff_python_stdlib::builtins;
use ruff_text_size::{Ranged, TextRange};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::fix::edits::pad;
use crate::preview::is_b014_builtin_exception_hierarchy_enabled;
use crate::registry::Rule;
use crate::{AlwaysFixableViolation, Violation};
use crate::{Edit, Fix};

/// ## What it does
/// Checks for `try-except` blocks with duplicate exception handlers.
///
/// ## Why is this bad?
/// Duplicate exception handlers are redundant, as the first handler will catch
/// the exception, making the second handler unreachable.
///
/// ## Example
/// ```python
/// try:
///     ...
/// except ValueError:
///     ...
/// except ValueError:
///     ...
/// ```
///
/// Use instead:
/// ```python
/// try:
///     ...
/// except ValueError:
///     ...
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as safe, unless the exception handler contains comments.
///
/// ## References
/// - [Python documentation: `except` clause](https://docs.python.org/3/reference/compound_stmts.html#except-clause)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.67", category = Category::Correctness)]
pub(crate) struct DuplicateTryBlockException {
    name: String,
    is_star: bool,
}

impl Violation for DuplicateTryBlockException {
    #[derive_message_formats]
    fn message(&self) -> String {
        let DuplicateTryBlockException { name, is_star } = self;
        if *is_star {
            format!("try-except* block with duplicate exception `{name}`")
        } else {
            format!("try-except block with duplicate exception `{name}`")
        }
    }
}

/// ## What it does
/// Checks for exception handlers that catch duplicate exceptions.
///
/// In [preview], also checks for redundant built-in exception subclasses, such
/// as `TimeoutError` in `except (OSError, TimeoutError)`.
/// Hierarchy checks exclude exception groups and tuples containing expressions
/// other than names and attribute accesses.
///
/// ## Why is this bad?
/// Including the same exception multiple times in the same handler is redundant,
/// as the first exception will catch the exception, making the second exception
/// unreachable. The same applies to exception hierarchies, as a handler for a
/// parent exception (like `Exception`) will also catch child exceptions (like
/// `ValueError`).
///
/// ## Example
/// ```python
/// try:
///     ...
/// except (ValueError, ValueError):
///     ...
/// ```
///
/// Use instead:
/// ```python
/// try:
///     ...
/// except ValueError:
///     ...
/// ```
///
/// ## References
/// - [Python documentation: `except` clause](https://docs.python.org/3/reference/compound_stmts.html#except-clause)
/// - [Python documentation: Exception hierarchy](https://docs.python.org/3/library/exceptions.html#exception-hierarchy)
///
/// [preview]: https://docs.astral.sh/ruff/preview/
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.67", category = Category::Correctness)]
pub(crate) struct DuplicateHandlerException {
    pub names: Vec<String>,
}

impl AlwaysFixableViolation for DuplicateHandlerException {
    #[derive_message_formats]
    fn message(&self) -> String {
        let DuplicateHandlerException { names } = self;
        if let [name] = names.as_slice() {
            format!("Exception handler with duplicate exception: `{name}`")
        } else {
            let names = names.iter().map(|name| format!("`{name}`")).join(", ");
            format!("Exception handler with duplicate exceptions: {names}")
        }
    }

    fn fix_title(&self) -> String {
        "De-duplicate exceptions".to_string()
    }
}

fn type_pattern(elts: Vec<&Expr>) -> Expr {
    ast::ExprTuple {
        elts: elts.into_iter().cloned().collect(),
        ctx: ExprContext::Load,
        range: TextRange::default(),
        node_index: ruff_python_ast::AtomicNodeIndex::NONE,
        parenthesized: true,
    }
    .into()
}

/// B014
fn duplicate_handler_exceptions<'a>(
    checker: &Checker,
    expr: &'a Expr,
    elts: &'a [Expr],
) -> FxHashMap<UnqualifiedName<'a>, &'a Expr> {
    let mut seen: FxHashMap<UnqualifiedName, &Expr> = FxHashMap::default();
    let mut duplicates: FxHashSet<UnqualifiedName> = FxHashSet::default();
    let mut unique_elts: Vec<&Expr> = Vec::default();
    for type_ in elts {
        if let Some(name) = UnqualifiedName::from_expr(type_) {
            if seen.contains_key(&name) {
                duplicates.insert(name);
            } else {
                seen.entry(name).or_insert(type_);
                unique_elts.push(type_);
            }
        }
    }

    if checker.is_rule_enabled(Rule::DuplicateHandlerException) {
        // TODO(charlie): Handle "BaseException" with custom exceptions and redundant exception aliases.
        // The existing fix only retains expressions that have syntactic names.
        // Do not introduce hierarchy fixes that would drop other expressions.
        if is_b014_builtin_exception_hierarchy_enabled(checker.settings())
            && elts
                .iter()
                .all(|elt| UnqualifiedName::from_expr(elt).is_some())
        {
            let builtin_exceptions: Vec<_> = unique_elts
                .iter()
                .filter_map(|elt| builtin_exception_name(checker, elt))
                .collect();
            unique_elts.retain(|elt| {
                if let Some(child) = builtin_exception_name(checker, elt)
                    && builtin_exceptions
                        .iter()
                        .any(|parent| is_exception_subclass(child, parent))
                    && let Some(name) = UnqualifiedName::from_expr(elt)
                {
                    duplicates.insert(name);
                    false
                } else {
                    true
                }
            });
        }
        if !duplicates.is_empty() {
            let mut diagnostic = checker.report_diagnostic(
                DuplicateHandlerException {
                    names: duplicates
                        .into_iter()
                        .map(|qualified_name| qualified_name.segments().join("."))
                        .sorted()
                        .collect::<Vec<String>>(),
                },
                expr.range(),
            );

            let applicability = if checker.comment_ranges().intersects(expr.range()) {
                Applicability::Unsafe
            } else {
                Applicability::Safe
            };

            diagnostic.set_fix(Fix::applicable_edit(
                Edit::range_replacement(
                    // Single exceptions don't require parentheses, but since we're _removing_
                    // parentheses, insert whitespace as needed.
                    if let [elt] = unique_elts.as_slice() {
                        pad(
                            checker.generator().expr(elt),
                            expr.range(),
                            checker.locator(),
                        )
                    } else {
                        // Multiple exceptions must always be parenthesized. This is done
                        // manually as the generator never parenthesizes lone tuples.
                        format!("({})", checker.generator().expr(&type_pattern(unique_elts)))
                    },
                    expr.range(),
                ),
                applicability,
            ));
        }
    }

    seen
}

fn builtin_exception_name<'a>(checker: &'a Checker, expr: &'a Expr) -> Option<&'a str> {
    let semantic = checker.semantic();
    let mut head = expr;
    while let Expr::Attribute(attribute) = head {
        head = &attribute.value;
    }
    let Expr::Name(head) = head else {
        return None;
    };
    // Resolution is cached before later function-local assignments are known, including
    // those in functions enclosing a class body. Stop at the resolved binding's scope:
    // an import initialized before this reference remains valid even if rebound later.
    let resolved_id = semantic.resolve_name(head)?;
    let resolved_scope = semantic.binding(resolved_id).scope;
    for scope_id in semantic.current_scope_ids() {
        if scope_id == resolved_scope {
            break;
        }
        let scope = &semantic.scopes[scope_id];
        if let Some(local_id) = scope.get(&head.id) {
            let local = semantic.binding(local_id);
            if local.is_global() {
                break;
            }
            if scope.kind.is_function() && !local.is_nonlocal() {
                return None;
            }
        }
    }
    let qualified_name = semantic.resolve_qualified_name(expr)?;
    match qualified_name.segments() {
        ["" | "builtins", name]
            if builtins::is_exception(name, checker.target_version().minor)
                && !matches!(*name, "BaseExceptionGroup" | "ExceptionGroup") =>
        {
            Some(*name)
        }
        _ => None,
    }
}

fn is_exception_subclass(mut child: &str, parent: &str) -> bool {
    while let Some(base) = exception_base(child) {
        if base == parent {
            return true;
        }
        child = base;
    }
    false
}

/// Direct bases of built-in exceptions, excluding exception groups and aliases.
/// See <https://docs.python.org/3/library/exceptions.html#exception-hierarchy>.
fn exception_base(name: &str) -> Option<&'static str> {
    Some(match name {
        "Exception" | "GeneratorExit" | "KeyboardInterrupt" | "SystemExit" => "BaseException",
        "ArithmeticError" | "AssertionError" | "AttributeError" | "BufferError" | "EOFError"
        | "ImportError" | "LookupError" | "MemoryError" | "NameError" | "OSError"
        | "ReferenceError" | "RuntimeError" | "StopAsyncIteration" | "StopIteration"
        | "SyntaxError" | "SystemError" | "TypeError" | "ValueError" | "Warning" => "Exception",
        "FloatingPointError" | "OverflowError" | "ZeroDivisionError" => "ArithmeticError",
        "ModuleNotFoundError" | "ImportCycleError" => "ImportError",
        "IndexError" | "KeyError" => "LookupError",
        "UnboundLocalError" => "NameError",
        "BlockingIOError" | "ChildProcessError" | "ConnectionError" | "FileExistsError"
        | "FileNotFoundError" | "InterruptedError" | "IsADirectoryError" | "NotADirectoryError"
        | "PermissionError" | "ProcessLookupError" | "TimeoutError" => "OSError",
        "BrokenPipeError"
        | "ConnectionAbortedError"
        | "ConnectionRefusedError"
        | "ConnectionResetError" => "ConnectionError",
        "NotImplementedError" | "PythonFinalizationError" | "RecursionError" => "RuntimeError",
        "IndentationError" => "SyntaxError",
        "TabError" => "IndentationError",
        "UnicodeError" => "ValueError",
        "UnicodeDecodeError" | "UnicodeEncodeError" | "UnicodeTranslateError" => "UnicodeError",
        "BytesWarning"
        | "DeprecationWarning"
        | "EncodingWarning"
        | "FutureWarning"
        | "ImportWarning"
        | "PendingDeprecationWarning"
        | "ResourceWarning"
        | "RuntimeWarning"
        | "SyntaxWarning"
        | "UnicodeWarning"
        | "UserWarning" => "Warning",
        _ => return None,
    })
}

/// B025
pub(crate) fn duplicate_exceptions(checker: &Checker, handlers: &[ExceptHandler]) {
    let mut seen: FxHashSet<UnqualifiedName> = FxHashSet::default();
    let mut duplicates: FxHashMap<UnqualifiedName, Vec<&Expr>> = FxHashMap::default();
    for handler in handlers {
        let ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler {
            type_: Some(type_),
            ..
        }) = handler
        else {
            continue;
        };
        match type_.as_ref() {
            Expr::Attribute(_) | Expr::Name(_) => {
                if let Some(name) = UnqualifiedName::from_expr(type_) {
                    if seen.contains(&name) {
                        duplicates.entry(name).or_default().push(type_);
                    } else {
                        seen.insert(name);
                    }
                }
            }
            Expr::Tuple(ast::ExprTuple { elts, .. }) => {
                #[expect(
                    clippy::iter_over_hash_type,
                    reason = "each distinct exception name updates independent set and map entries"
                )]
                for (name, expr) in duplicate_handler_exceptions(checker, type_, elts) {
                    if seen.contains(&name) {
                        duplicates.entry(name).or_default().push(expr);
                    } else {
                        seen.insert(name);
                    }
                }
            }
            _ => {}
        }
    }

    if checker.is_rule_enabled(Rule::DuplicateTryBlockException) {
        #[expect(
            clippy::iter_over_hash_type,
            reason = "iteration order does not affect the diagnostics produced"
        )]
        for (name, exprs) in duplicates {
            for expr in exprs {
                let is_star = checker
                    .semantic()
                    .current_statement()
                    .as_try_stmt()
                    .is_some_and(|try_stmt| try_stmt.is_star);
                checker.report_diagnostic(
                    DuplicateTryBlockException {
                        name: name.segments().join("."),
                        is_star,
                    },
                    expr.range(),
                );
            }
        }
    }
}
