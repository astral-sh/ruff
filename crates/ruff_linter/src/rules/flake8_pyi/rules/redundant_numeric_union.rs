use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{AnyParameterRef, Expr, Parameters};
use ruff_python_semantic::analyze::typing::traverse_union;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for parameter annotations that contain redundant unions between
/// builtin numeric types (e.g., `int | float`).
///
/// ## Why is this bad?
/// The [typing specification] states:
///
/// > Python’s numeric types `complex`, `float` and `int` are not subtypes of
/// > each other, but to support common use cases, the type system contains a
/// > straightforward shortcut: when an argument is annotated as having type
/// > `float`, an argument of type `int` is acceptable; similar, for an
/// > argument annotated as having type `complex`, arguments of type `float` or
/// > `int` are acceptable.
///
/// As such, a union that includes both `int` and `float` is redundant in the
/// specific context of a parameter annotation, as it is equivalent to a union
/// that only includes `float`. For readability and clarity, unions should omit
/// redundant elements.
///
/// ## Example
///
/// ```pyi
/// def foo(x: float | int | str) -> None: ...
/// ```
///
/// Use instead:
///
/// ```pyi
/// def foo(x: float | str) -> None: ...
/// ```
///
/// ## References
/// - [The typing specification](https://docs.python.org/3/library/numbers.html#the-numeric-tower)
/// - [PEP 484: The numeric tower](https://peps.python.org/pep-0484/#the-numeric-tower)
///
/// [typing specification]: https://typing.readthedocs.io/en/latest/spec/special-types.html#special-cases-for-float-and-complex
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
    for annotation in parameters.iter().filter_map(AnyParameterRef::annotation) {
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

    let mut find_numeric_type = |expr: &Expr, _parent: &Expr| {
        let Some(builtin_type) = checker.semantic().resolve_builtin_symbol(expr) else {
            return;
        };

        match builtin_type {
            "int" => has_int = true,
            "float" => has_float = true,
            "complex" => has_complex = true,
            _ => {}
        }
    };

    traverse_union(&mut find_numeric_type, checker.semantic(), annotation);

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
