use ruff_python_ast::{self as ast, ExceptHandler, Expr, ExprContext};
use ruff_text_size::{Ranged, TextRange};

use crate::fix::edits::pad;
use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::name::{Name, UnqualifiedName};
use ruff_python_semantic::SemanticModel;

use crate::checkers::ast::Checker;
use ruff_python_ast::PythonVersion;

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
#[derive(ViolationMetadata)]
pub(crate) struct TimeoutErrorAlias {
    name: Option<String>,
}

impl AlwaysFixableViolation for TimeoutErrorAlias {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Replace aliased errors with `TimeoutError`".to_string()
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
    semantic
        .resolve_qualified_name(expr)
        .is_some_and(|qualified_name| {
            if target_version >= PythonVersion::PY311 {
                matches!(
                    qualified_name.segments(),
                    ["socket", "timeout"] | ["asyncio", "TimeoutError"]
                )
            } else {
                // N.B. This lint is only invoked for Python 3.10+. We assume
                // as much here since otherwise socket.timeout would be an unsafe
                // fix in Python <3.10. We add an assert to make this assumption
                // explicit.
                assert!(
                    target_version >= PythonVersion::PY310,
                    "lint should only be used for Python 3.10+",
                );
                matches!(qualified_name.segments(), ["socket", "timeout"])
            }
        })
}

/// Create a [`Diagnostic`] for a single target, like an [`Expr::Name`].
fn atom_diagnostic(checker: &Checker, target: &Expr) {
    let mut diagnostic = Diagnostic::new(
        TimeoutErrorAlias {
            name: UnqualifiedName::from_expr(target).map(|name| name.to_string()),
        },
        target.range(),
    );
    diagnostic.try_set_fix(|| {
        let (import_edit, binding) = checker.importer().get_or_import_builtin_symbol(
            "TimeoutError",
            target.start(),
            checker.semantic(),
        )?;
        Ok(Fix::safe_edits(
            Edit::range_replacement(binding, target.range()),
            import_edit,
        ))
    });
    checker.report_diagnostic(diagnostic);
}

/// Create a [`Diagnostic`] for a tuple of expressions.
fn tuple_diagnostic(checker: &Checker, tuple: &ast::ExprTuple, aliases: &[&Expr]) {
    let mut diagnostic = Diagnostic::new(TimeoutErrorAlias { name: None }, tuple.range());
    let semantic = checker.semantic();
    if semantic.has_builtin_binding("TimeoutError") {
        // Filter out any `TimeoutErrors` aliases.
        let mut remaining: Vec<Expr> = tuple
            .iter()
            .filter_map(|element| {
                if aliases.contains(&element) {
                    None
                } else {
                    Some(element.clone())
                }
            })
            .collect();

        // If `TimeoutError` itself isn't already in the tuple, add it.
        if tuple
            .iter()
            .all(|element| !semantic.match_builtin_expr(element, "TimeoutError"))
        {
            let node = ast::ExprName {
                id: Name::new_static("TimeoutError"),
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
                parenthesized: true,
            };
            format!("({})", checker.generator().expr(&node.into()))
        };

        diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
            pad(content, tuple.range(), checker.locator()),
            tuple.range(),
        )));
    }
    checker.report_diagnostic(diagnostic);
}

/// UP041
pub(crate) fn timeout_error_alias_handlers(checker: &Checker, handlers: &[ExceptHandler]) {
    for handler in handlers {
        let ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler { type_, .. }) = handler;
        let Some(expr) = type_.as_ref() else {
            continue;
        };
        match expr.as_ref() {
            Expr::Name(_) | Expr::Attribute(_) => {
                if is_alias(expr, checker.semantic(), checker.target_version()) {
                    atom_diagnostic(checker, expr);
                }
            }
            Expr::Tuple(tuple) => {
                // List of aliases to replace with `TimeoutError`.
                let mut aliases: Vec<&Expr> = vec![];
                for element in tuple {
                    if is_alias(element, checker.semantic(), checker.target_version()) {
                        aliases.push(element);
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
pub(crate) fn timeout_error_alias_call(checker: &Checker, func: &Expr) {
    if is_alias(func, checker.semantic(), checker.target_version()) {
        atom_diagnostic(checker, func);
    }
}

/// UP041
pub(crate) fn timeout_error_alias_raise(checker: &Checker, expr: &Expr) {
    if matches!(expr, Expr::Name(_) | Expr::Attribute(_)) {
        if is_alias(expr, checker.semantic(), checker.target_version()) {
            atom_diagnostic(checker, expr);
        }
    }
}
