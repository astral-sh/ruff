use rustpython_parser::ast::{Arguments, Expr, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;
use crate::rules::flake8_pyi::helpers::traverse_union;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum Redundancy {
    FloatComplex,
    IntComplex,
    IntFloat,
}

/// ## What it does
/// Checks for unions in function parameter annotations that contain redundant numeric types.
/// See PEP 3141 for details on the "numeric tower".
///
/// ## Why is this bad?
/// Unions with redundant elements are less readable than unions without them.
///
/// ## Example
/// ```python
/// def foo(arg: float | int) -> None:
///     ...
/// ```
///
/// Use instead:
/// ```python
/// def foo(arg: float) -> None:
///     ...
/// ```
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

        format!("`{subtype}` is redundant in a union with `{supertype}`.")
    }
}

fn check_annotation(annotation: &Expr, checker: &mut Checker) {
    let mut has_float = false;
    let mut has_complex = false;
    let mut has_int = false;

    let mut eval_element = |expr: &Expr, _parent: Option<&Expr>| {
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

    traverse_union(&mut eval_element, checker.semantic(), annotation, None);

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

/// PYI041
pub(crate) fn redundant_numeric_union(checker: &mut Checker, args: &Arguments) {
    let annotations = args
        .args
        .iter()
        .chain(args.posonlyargs.iter())
        .chain(args.kwonlyargs.iter())
        .filter_map(|arg| arg.def.annotation.as_ref());

    for annotation in annotations {
        check_annotation(annotation, checker);
    }

    // If annotations on `args` or `kwargs` are flagged by this rule, the annotations themselves
    // are not accurate, but check them anyway. It's possible that flagging them will help the user
    // realize they're incorrect.
    let args_kwargs_annotations = args
        .vararg
        .iter()
        .chain(args.kwarg.iter())
        .filter_map(|arg| arg.annotation.as_ref());

    for annotation in args_kwargs_annotations {
        check_annotation(annotation, checker);
    }
}
