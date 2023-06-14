use ruff_text_size::TextRange;
use rustpython_parser::ast::{self, Excepthandler, Expr, ExprContext, Ranged};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::call_path::compose_call_path;
use ruff_python_semantic::model::SemanticModel;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

/// ## What it does
/// Checks for `OSError` aliases.
///
/// ## Why is this bad?
/// `OSError` is the builtin error type for exceptions related to the operating
/// system. Replacing aliases to `OSError` with `OSError` makes the exception
/// more explicit and is more idiomatic.
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
    pub name: Option<String>,
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

const ALIASES: &[(&str, &str)] = &[
    ("", "EnvironmentError"),
    ("", "IOError"),
    ("", "WindowsError"),
    ("mmap", "error"),
    ("select", "error"),
    ("socket", "error"),
];

/// Return `true` if an [`Expr`] is an alias of `OSError`.
fn is_alias(model: &SemanticModel, expr: &Expr) -> bool {
    model.resolve_call_path(expr).map_or(false, |call_path| {
        ALIASES
            .iter()
            .any(|(module, member)| call_path.as_slice() == [*module, *member])
    })
}

/// Return `true` if an [`Expr`] is `OSError`.
fn is_os_error(model: &SemanticModel, expr: &Expr) -> bool {
    model
        .resolve_call_path(expr)
        .map_or(false, |call_path| call_path.as_slice() == ["", "OSError"])
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
        #[allow(deprecated)]
        diagnostic.set_fix(Fix::unspecified(Edit::range_replacement(
            "OSError".to_string(),
            target.range(),
        )));
    }
    checker.diagnostics.push(diagnostic);
}

/// Create a [`Diagnostic`] for a tuple of expressions.
fn tuple_diagnostic(checker: &mut Checker, target: &Expr, aliases: &[&Expr]) {
    let mut diagnostic = Diagnostic::new(OSErrorAlias { name: None }, target.range());
    if checker.patch(diagnostic.kind.rule()) {
        let Expr::Tuple(ast::ExprTuple { elts, ..}) = target else {
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
        if elts
            .iter()
            .all(|elt| !is_os_error(checker.semantic_model(), elt))
        {
            let node = ast::ExprName {
                id: "OSError".into(),
                ctx: ExprContext::Load,
                range: TextRange::default(),
            };
            remaining.insert(0, node.into());
        }

        if remaining.len() == 1 {
            #[allow(deprecated)]
            diagnostic.set_fix(Fix::unspecified(Edit::range_replacement(
                "OSError".to_string(),
                target.range(),
            )));
        } else {
            let node = ast::ExprTuple {
                elts: remaining,
                ctx: ExprContext::Load,
                range: TextRange::default(),
            };
            #[allow(deprecated)]
            diagnostic.set_fix(Fix::unspecified(Edit::range_replacement(
                format!("({})", checker.generator().expr(&node.into())),
                target.range(),
            )));
        }
    }
    checker.diagnostics.push(diagnostic);
}

/// UP024
pub(crate) fn os_error_alias_handlers(checker: &mut Checker, handlers: &[Excepthandler]) {
    for handler in handlers {
        let Excepthandler::ExceptHandler(ast::ExcepthandlerExceptHandler { type_, .. }) = handler;
        let Some(expr) = type_.as_ref() else {
            continue;
        };
        match expr.as_ref() {
            Expr::Name(_) | Expr::Attribute(_) => {
                if is_alias(checker.semantic_model(), expr) {
                    atom_diagnostic(checker, expr);
                }
            }
            Expr::Tuple(ast::ExprTuple { elts, .. }) => {
                // List of aliases to replace with `OSError`.
                let mut aliases: Vec<&Expr> = vec![];
                for elt in elts {
                    if is_alias(checker.semantic_model(), elt) {
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
    if is_alias(checker.semantic_model(), func) {
        atom_diagnostic(checker, func);
    }
}

/// UP024
pub(crate) fn os_error_alias_raise(checker: &mut Checker, expr: &Expr) {
    if matches!(expr, Expr::Name(_) | Expr::Attribute(_)) {
        if is_alias(checker.semantic_model(), expr) {
            atom_diagnostic(checker, expr);
        }
    }
}
