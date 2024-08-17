use std::fmt;

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::pep_604_union;
use ruff_python_ast::Expr;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub(crate) enum CallKind {
    Isinstance,
    Issubclass,
}

impl fmt::Display for CallKind {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            CallKind::Isinstance => fmt.write_str("isinstance"),
            CallKind::Issubclass => fmt.write_str("issubclass"),
        }
    }
}

impl CallKind {
    pub(crate) fn from_name(name: &str) -> Option<Self> {
        match name {
            "isinstance" => Some(CallKind::Isinstance),
            "issubclass" => Some(CallKind::Issubclass),
            _ => None,
        }
    }
}

/// ## What it does
/// Checks for uses of `isinstance` and `issubclass` that take a tuple
/// of types for comparison.
///
/// ## Why is this bad?
/// Since Python 3.10, `isinstance` and `issubclass` can be passed a
/// `|`-separated union of types, which is consistent
/// with the union operator introduced in [PEP 604].
///
/// Note that this results in slower code. Ignore this rule if the
/// performance of an `isinstance` or `issubclass` check is a
/// concern, e.g., in a hot loop.
///
/// ## Example
/// ```python
/// isinstance(x, (int, float))
/// ```
///
/// Use instead:
/// ```python
/// isinstance(x, int | float)
/// ```
///
/// ## Options
/// - `target-version`
///
/// ## References
/// - [Python documentation: `isinstance`](https://docs.python.org/3/library/functions.html#isinstance)
/// - [Python documentation: `issubclass`](https://docs.python.org/3/library/functions.html#issubclass)
///
/// [PEP 604]: https://peps.python.org/pep-0604/
#[violation]
pub struct NonPEP604Isinstance {
    kind: CallKind,
}

impl AlwaysFixableViolation for NonPEP604Isinstance {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `X | Y` in `{}` call instead of `(X, Y)`", self.kind)
    }

    fn fix_title(&self) -> String {
        "Convert to `X | Y`".to_string()
    }
}

/// UP038
pub(crate) fn use_pep604_isinstance(
    checker: &mut Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
) {
    let Some(types) = args.get(1) else {
        return;
    };
    let Expr::Tuple(tuple) = types else {
        return;
    };
    // Ex) `()`
    if tuple.is_empty() {
        return;
    }
    // Ex) `(*args,)`
    if tuple.iter().any(Expr::is_starred_expr) {
        return;
    }
    let Some(builtin_function_name) = checker.semantic().resolve_builtin_symbol(func) else {
        return;
    };
    let Some(kind) = CallKind::from_name(builtin_function_name) else {
        return;
    };
    checker.diagnostics.push(
        Diagnostic::new(NonPEP604Isinstance { kind }, expr.range()).with_fix(Fix::unsafe_edit(
            Edit::range_replacement(
                checker.generator().expr(&pep_604_union(&tuple.elts)),
                types.range(),
            ),
        )),
    );
}
