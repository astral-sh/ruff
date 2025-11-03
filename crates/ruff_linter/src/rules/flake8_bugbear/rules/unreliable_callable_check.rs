use ruff_diagnostics::Applicability;
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, Expr};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::{Edit, Fix, FixAvailability, Violation};

/// ## What it does
/// Checks for uses of `hasattr` to test if an object is callable (e.g.,
/// `hasattr(obj, "__call__")`).
///
/// ## Why is this bad?
/// Using `hasattr` is an unreliable mechanism for testing if an object is
/// callable. If `obj` implements a custom `__getattr__`, or if its `__call__`
/// is itself not callable, you may get misleading results.
///
/// Instead, use `callable(obj)` to test if `obj` is callable.
///
/// ## Example
/// ```python
/// hasattr(obj, "__call__")
/// ```
///
/// Use instead:
/// ```python
/// callable(obj)
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as unsafe because the replacement may not be semantically
/// equivalent to the original expression, potentially changing the behavior of the code.
///
/// For example, an imported module may have a `__call__` attribute but is not considered
/// a callable object:
/// ```python
/// import operator
///
/// assert hasattr(operator, "__call__")
/// assert callable(operator) is False
/// ```
/// Additionally, `__call__` may be defined only as an instance method:
/// ```python
/// class A:
///     def __init__(self):
///         self.__call__ = None
///
///
/// assert hasattr(A(), "__call__")
/// assert callable(A()) is False
/// ```
///
/// Additionally, if there are comments in the `hasattr` call expression, they may be removed:
/// ```python
/// hasattr(
///     # comment 1
///     obj,  # comment 2
///     # comment 3
///     "__call__",  # comment 4
///     # comment 5
/// )
/// ```
///
/// ## References
/// - [Python documentation: `callable`](https://docs.python.org/3/library/functions.html#callable)
/// - [Python documentation: `hasattr`](https://docs.python.org/3/library/functions.html#hasattr)
/// - [Python documentation: `__getattr__`](https://docs.python.org/3/reference/datamodel.html#object.__getattr__)
/// - [Python documentation: `__call__`](https://docs.python.org/3/reference/datamodel.html#object.__call__)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.106")]
pub(crate) struct UnreliableCallableCheck;

impl Violation for UnreliableCallableCheck {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Using `hasattr(x, \"__call__\")` to test if x is callable is unreliable. Use \
             `callable(x)` for consistent results."
            .to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Replace with `callable()`".to_string())
    }
}

/// B004
pub(crate) fn unreliable_callable_check(
    checker: &Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
    keywords: &[ast::Keyword],
) {
    if !keywords.is_empty() {
        return;
    }
    let [obj, attr, ..] = args else {
        return;
    };
    let Expr::StringLiteral(ast::ExprStringLiteral { value, .. }) = attr else {
        return;
    };
    if value != "__call__" {
        return;
    }
    let Some(builtins_function) = checker.semantic().resolve_builtin_symbol(func) else {
        return;
    };

    // Validate function arguments based on function name
    let valid_args = match builtins_function {
        "hasattr" => {
            // hasattr should have exactly 2 positional arguments and no keywords
            args.len() == 2
        }
        "getattr" => {
            // getattr should have 2 or 3 positional arguments and no keywords
            args.len() == 2 || args.len() == 3
        }
        _ => return,
    };

    if !valid_args {
        return;
    }

    let mut diagnostic = checker.report_diagnostic(UnreliableCallableCheck, expr.range());
    if builtins_function == "hasattr" {
        diagnostic.try_set_fix(|| {
            let (import_edit, binding) = checker.importer().get_or_import_builtin_symbol(
                "callable",
                expr.start(),
                checker.semantic(),
            )?;
            let binding_edit = Edit::range_replacement(
                format!("{binding}({})", checker.locator().slice(obj)),
                expr.range(),
            );
            Ok(Fix::applicable_edits(
                binding_edit,
                import_edit,
                Applicability::Unsafe,
            ))
        });
    }
}
