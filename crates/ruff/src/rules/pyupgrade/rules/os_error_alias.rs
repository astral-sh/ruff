use ruff_python_ast::{self as ast, ExceptHandler, Expr, ExprContext};
use ruff_text_size::{Ranged, TextRange};

use crate::autofix::edits::pad;
use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::call_path::compose_call_path;
use ruff_python_semantic::SemanticModel;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

/// ## What it does
/// Checks for uses of exceptions that alias `OSError`.
///
/// ## Why is this bad?
/// `OSError` is the builtin error type used for exceptions that relate to the
/// operating system.
///
/// In Python 3.3, a variety of other exceptions, like `WindowsError` were
/// aliased to `OSError`. These aliases remain in place for compatibility with
/// older versions of Python, but may be removed in future versions.
///
/// Prefer using `OSError` directly, as it is more idiomatic and future-proof.
///
/// ## Example
/// ```python
/// raise IOError
/// ```
///
/// Use instead:
/// ```python
/// raise OSError
/// ```
///
/// ## References
/// - [Python documentation: `OSError`](https://docs.python.org/3/library/exceptions.html#OSError)
#[violation]
pub struct OSErrorAlias {
    name: Option<String>,
}

impl AlwaysAutofixableViolation for OSErrorAlias {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Replace aliased errors with `OSError`")
    }

    fn autofix_title(&self) -> String {
        let OSErrorAlias { name } = self;
        match name {
            None => "Replace with builtin `OSError`".to_string(),
            Some(name) => format!("Replace `{name}` with builtin `OSError`"),
        }
    }
}

/// Return `true` if an [`Expr`] is an alias of `OSError`.
fn is_alias(expr: &Expr, semantic: &SemanticModel) -> bool {
    semantic.resolve_call_path(expr).is_some_and(|call_path| {
        matches!(
            call_path.as_slice(),
            ["", "EnvironmentError" | "IOError" | "WindowsError"]
                | ["mmap" | "select" | "socket", "error"]
        )
    })
}

/// Return `true` if an [`Expr`] is `OSError`.
fn is_os_error(expr: &Expr, semantic: &SemanticModel) -> bool {
    semantic
        .resolve_call_path(expr)
        .is_some_and(|call_path| matches!(call_path.as_slice(), ["", "OSError"]))
}

/// Create a [`Diagnostic`] for a single target, like an [`Expr::Name`].
fn atom_diagnostic(checker: &mut Checker, target: &Expr) {
    let mut diagnostic = Diagnostic::new(
        OSErrorAlias {
            name: compose_call_path(target),
        },
        target.range(),
    );
    if checker.patch(diagnostic.kind.rule()) {
        if checker.semantic().is_builtin("OSError") {
            diagnostic.set_fix(Fix::automatic(Edit::range_replacement(
                "OSError".to_string(),
                target.range(),
            )));
        }
    }
    checker.diagnostics.push(diagnostic);
}

/// Create a [`Diagnostic`] for a tuple of expressions.
fn tuple_diagnostic(checker: &mut Checker, target: &Expr, aliases: &[&Expr]) {
    let mut diagnostic = Diagnostic::new(OSErrorAlias { name: None }, target.range());
    if checker.patch(diagnostic.kind.rule()) {
        if checker.semantic().is_builtin("OSError") {
            let Expr::Tuple(ast::ExprTuple { elts, .. }) = target else {
                panic!("Expected Expr::Tuple");
            };

            // Filter out any `OSErrors` aliases.
            let mut remaining: Vec<Expr> = elts
                .iter()
                .filter_map(|elt| {
                    if aliases.contains(&elt) {
                        None
                    } else {
                        Some(elt.clone())
                    }
                })
                .collect();

            // If `OSError` itself isn't already in the tuple, add it.
            if elts.iter().all(|elt| !is_os_error(elt, checker.semantic())) {
                let node = ast::ExprName {
                    id: "OSError".into(),
                    ctx: ExprContext::Load,
                    range: TextRange::default(),
                };
                remaining.insert(0, node.into());
            }

            let content = if remaining.len() == 1 {
                "OSError".to_string()
            } else {
                let node = ast::ExprTuple {
                    elts: remaining,
                    ctx: ExprContext::Load,
                    range: TextRange::default(),
                };
                format!("({})", checker.generator().expr(&node.into()))
            };

            diagnostic.set_fix(Fix::automatic(Edit::range_replacement(
                pad(content, target.range(), checker.locator()),
                target.range(),
            )));
        }
    }
    checker.diagnostics.push(diagnostic);
}

/// UP024
pub(crate) fn os_error_alias_handlers(checker: &mut Checker, handlers: &[ExceptHandler]) {
    for handler in handlers {
        let ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler { type_, .. }) = handler;
        let Some(expr) = type_.as_ref() else {
            continue;
        };
        match expr.as_ref() {
            Expr::Name(_) | Expr::Attribute(_) => {
                if is_alias(expr, checker.semantic()) {
                    atom_diagnostic(checker, expr);
                }
            }
            Expr::Tuple(ast::ExprTuple { elts, .. }) => {
                // List of aliases to replace with `OSError`.
                let mut aliases: Vec<&Expr> = vec![];
                for elt in elts {
                    if is_alias(elt, checker.semantic()) {
                        aliases.push(elt);
                    }
                }
                if !aliases.is_empty() {
                    tuple_diagnostic(checker, expr, &aliases);
                }
            }
            _ => {}
        }
    }
}

/// UP024
pub(crate) fn os_error_alias_call(checker: &mut Checker, func: &Expr) {
    if is_alias(func, checker.semantic()) {
        atom_diagnostic(checker, func);
    }
}

/// UP024
pub(crate) fn os_error_alias_raise(checker: &mut Checker, expr: &Expr) {
    if matches!(expr, Expr::Name(_) | Expr::Attribute(_)) {
        if is_alias(expr, checker.semantic()) {
            atom_diagnostic(checker, expr);
        }
    }
}
