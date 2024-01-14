use ruff_python_ast::{self as ast, ExceptHandler, Expr, ExprContext};
use ruff_text_size::{Ranged, TextRange};

use crate::fix::edits::pad;
use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::call_path::compose_call_path;
use ruff_python_semantic::SemanticModel;

use crate::checkers::ast::Checker;
use crate::settings::types::PythonVersion;

/// ## What it does
/// Checks for uses of exceptions that alias `TimeoutError`.
///
/// ## Why is this bad?
/// `TimeoutError` is the builtin error type used for exceptions when a system
/// function timed out at the system level.
///
/// In Python 3.10, `socket.timeout` was aliased to `TimeoutError`. In Python
/// 3.11, `asyncio.TimeoutError` was aliased to `TimeoutError`.
///
/// These aliases remain in place for compatibility with older versions of
/// Python, but may be removed in future versions.
///
/// Prefer using `TimeoutError` directly, as it is more idiomatic and future-proof.
///
/// ## Example
/// ```python
/// raise asyncio.TimeoutError
/// ```
///
/// Use instead:
/// ```python
/// raise TimeoutError
/// ```
///
/// ## References
/// - [Python documentation: `TimeoutError`](https://docs.python.org/3/library/exceptions.html#TimeoutError)
#[violation]
pub struct TimeoutErrorAlias {
    name: Option<String>,
}

impl AlwaysFixableViolation for TimeoutErrorAlias {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Replace aliased errors with `TimeoutError`")
    }

    fn fix_title(&self) -> String {
        let TimeoutErrorAlias { name } = self;
        match name {
            None => "Replace with builtin `TimeoutError`".to_string(),
            Some(name) => format!("Replace `{name}` with builtin `TimeoutError`"),
        }
    }
}

/// Return `true` if an [`Expr`] is an alias of `TimeoutError`.
fn is_alias(expr: &Expr, semantic: &SemanticModel, target_version: PythonVersion) -> bool {
    semantic.resolve_call_path(expr).is_some_and(|call_path| {
        if target_version >= PythonVersion::Py311 {
            matches!(
                call_path.as_slice(),
                ["socket", "timeout"] | ["asyncio", "TimeoutError"]
            )
        } else {
            // N.B. This lint is only invoked for Python 3.10+. We assume
            // as much here since otherwise socket.timeout would be an unsafe
            // fix in Python <3.10. We add an assert to make this assumption
            // explicit.
            assert!(
                target_version >= PythonVersion::Py310,
                "lint should only be used for Python 3.10+",
            );
            matches!(call_path.as_slice(), ["socket", "timeout"])
        }
    })
}

/// Return `true` if an [`Expr`] is `TimeoutError`.
fn is_timeout_error(expr: &Expr, semantic: &SemanticModel) -> bool {
    semantic
        .resolve_call_path(expr)
        .is_some_and(|call_path| matches!(call_path.as_slice(), ["", "TimeoutError"]))
}

/// Create a [`Diagnostic`] for a single target, like an [`Expr::Name`].
fn atom_diagnostic(checker: &mut Checker, target: &Expr) {
    let mut diagnostic = Diagnostic::new(
        TimeoutErrorAlias {
            name: compose_call_path(target),
        },
        target.range(),
    );
    if checker.semantic().is_builtin("TimeoutError") {
        diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
            "TimeoutError".to_string(),
            target.range(),
        )));
    }
    checker.diagnostics.push(diagnostic);
}

/// Create a [`Diagnostic`] for a tuple of expressions.
fn tuple_diagnostic(checker: &mut Checker, tuple: &ast::ExprTuple, aliases: &[&Expr]) {
    let mut diagnostic = Diagnostic::new(TimeoutErrorAlias { name: None }, tuple.range());
    if checker.semantic().is_builtin("TimeoutError") {
        // Filter out any `TimeoutErrors` aliases.
        let mut remaining: Vec<Expr> = tuple
            .elts
            .iter()
            .filter_map(|elt| {
                if aliases.contains(&elt) {
                    None
                } else {
                    Some(elt.clone())
                }
            })
            .collect();

        // If `TimeoutError` itself isn't already in the tuple, add it.
        if tuple
            .elts
            .iter()
            .all(|elt| !is_timeout_error(elt, checker.semantic()))
        {
            let node = ast::ExprName {
                id: "TimeoutError".into(),
                ctx: ExprContext::Load,
                range: TextRange::default(),
            };
            remaining.insert(0, node.into());
        }

        let content = if remaining.len() == 1 {
            "TimeoutError".to_string()
        } else {
            let node = ast::ExprTuple {
                elts: remaining,
                ctx: ExprContext::Load,
                range: TextRange::default(),
            };
            format!("({})", checker.generator().expr(&node.into()))
        };

        diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
            pad(content, tuple.range(), checker.locator()),
            tuple.range(),
        )));
    }
    checker.diagnostics.push(diagnostic);
}

/// UP041
pub(crate) fn timeout_error_alias_handlers(checker: &mut Checker, handlers: &[ExceptHandler]) {
    for handler in handlers {
        let ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler { type_, .. }) = handler;
        let Some(expr) = type_.as_ref() else {
            continue;
        };
        match expr.as_ref() {
            Expr::Name(_) | Expr::Attribute(_) => {
                if is_alias(expr, checker.semantic(), checker.settings.target_version) {
                    atom_diagnostic(checker, expr);
                }
            }
            Expr::Tuple(tuple) => {
                // List of aliases to replace with `TimeoutError`.
                let mut aliases: Vec<&Expr> = vec![];
                for elt in &tuple.elts {
                    if is_alias(elt, checker.semantic(), checker.settings.target_version) {
                        aliases.push(elt);
                    }
                }
                if !aliases.is_empty() {
                    tuple_diagnostic(checker, tuple, &aliases);
                }
            }
            _ => {}
        }
    }
}

/// UP041
pub(crate) fn timeout_error_alias_call(checker: &mut Checker, func: &Expr) {
    if is_alias(func, checker.semantic(), checker.settings.target_version) {
        atom_diagnostic(checker, func);
    }
}

/// UP041
pub(crate) fn timeout_error_alias_raise(checker: &mut Checker, expr: &Expr) {
    if matches!(expr, Expr::Name(_) | Expr::Attribute(_)) {
        if is_alias(expr, checker.semantic(), checker.settings.target_version) {
            atom_diagnostic(checker, expr);
        }
    }
}
