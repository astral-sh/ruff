use ruff_python_ast::{Expr, Parameters};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::rules::flake8_pyi::helpers::traverse_union;

/// ## What it does
/// Checks for union annotations that contain redundant numeric types (e.g.,
/// `int | float`).
///
/// ## Why is this bad?
/// In Python, `int` is a subtype of `float`, and `float` is a subtype of
/// `complex`. As such, a union that includes both `int` and `float` is
/// redundant, as it is equivalent to a union that only includes `float`.
///
/// For more, see [PEP 3141], which defines Python's "numeric tower".
///
/// Unions with redundant elements are less readable than unions without them.
///
/// ## Example
/// ```python
/// def foo(x: float | int) -> None:
///     ...
/// ```
///
/// Use instead:
/// ```python
/// def foo(x: float) -> None:
///     ...
/// ```
///
/// ## References
/// - [Python documentation: The numeric tower](https://docs.python.org/3/library/numbers.html#the-numeric-tower)
///
/// [PEP 3141]: https://peps.python.org/pep-3141/
#[violation]
pub struct RedundantNumericUnion {
    redundancy: Redundancy,
}

impl Violation for RedundantNumericUnion {
    #[derive_message_formats]
    fn message(&self) -> String {
        let (subtype, supertype) = match self.redundancy {
            Redundancy::FloatComplex => ("float", "complex"),
            Redundancy::IntComplex => ("int", "complex"),
            Redundancy::IntFloat => ("int", "float"),
        };
        format!("Use `{supertype}` instead of `{subtype} | {supertype}`")
    }
}

/// PYI041
pub(crate) fn redundant_numeric_union(checker: &mut Checker, parameters: &Parameters) {
    for annotation in parameters
        .args
        .iter()
        .chain(parameters.posonlyargs.iter())
        .chain(parameters.kwonlyargs.iter())
        .filter_map(|arg| arg.parameter.annotation.as_ref())
    {
        check_annotation(checker, annotation);
    }

    // If annotations on `args` or `kwargs` are flagged by this rule, the annotations themselves
    // are not accurate, but check them anyway. It's possible that flagging them will help the user
    // realize they're incorrect.
    for annotation in parameters
        .vararg
        .iter()
        .chain(parameters.kwarg.iter())
        .filter_map(|arg| arg.annotation.as_ref())
    {
        check_annotation(checker, annotation);
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum Redundancy {
    FloatComplex,
    IntComplex,
    IntFloat,
}

fn check_annotation(checker: &mut Checker, annotation: &Expr) {
    let mut has_float = false;
    let mut has_complex = false;
    let mut has_int = false;

    let mut func = |expr: &Expr, _parent: Option<&Expr>| {
        let Some(call_path) = checker.semantic().resolve_call_path(expr) else {
            return;
        };

        match call_path.as_slice() {
            ["" | "builtins", "int"] => has_int = true,
            ["" | "builtins", "float"] => has_float = true,
            ["" | "builtins", "complex"] => has_complex = true,
            _ => (),
        }
    };

    traverse_union(&mut func, checker.semantic(), annotation, None);

    if has_complex {
        if has_float {
            checker.diagnostics.push(Diagnostic::new(
                RedundantNumericUnion {
                    redundancy: Redundancy::FloatComplex,
                },
                annotation.range(),
            ));
        }

        if has_int {
            checker.diagnostics.push(Diagnostic::new(
                RedundantNumericUnion {
                    redundancy: Redundancy::IntComplex,
                },
                annotation.range(),
            ));
        }
    } else if has_float && has_int {
        checker.diagnostics.push(Diagnostic::new(
            RedundantNumericUnion {
                redundancy: Redundancy::IntFloat,
            },
            annotation.range(),
        ));
    }
}
