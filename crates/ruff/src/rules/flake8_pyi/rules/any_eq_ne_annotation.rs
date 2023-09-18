use ruff_python_ast::Parameters;

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

/// ## What it does
/// Checks for `__eq__` and `__ne__` implementations that use `typing.Any` as
/// the type annotation for the `obj` parameter.
///
/// ## Why is this bad?
/// The Python documentation recommends the use of `object` to "indicate that a
/// value could be any type in a typesafe manner", while `Any` should be used to
/// "indicate that a value is dynamically typed."
///
/// The semantics of `__eq__` and `__ne__` are such that the `obj` parameter
/// should be any type, as opposed to a dynamically typed value. Therefore, the
/// `object` type annotation is more appropriate.
///
/// ## Example
/// ```python
/// class Foo:
///     def __eq__(self, obj: typing.Any):
///         ...
/// ```
///
/// Use instead:
/// ```python
/// class Foo:
///     def __eq__(self, obj: object):
///         ...
/// ```
/// ## References
/// - [Python documentation: The `Any` type](https://docs.python.org/3/library/typing.html#the-any-type)
/// - [Mypy documentation](https://mypy.readthedocs.io/en/latest/dynamic_typing.html#any-vs-object)
#[violation]
pub struct AnyEqNeAnnotation {
    method_name: String,
}

impl AlwaysAutofixableViolation for AnyEqNeAnnotation {
    #[derive_message_formats]
    fn message(&self) -> String {
        let AnyEqNeAnnotation { method_name } = self;
        format!("Prefer `object` to `Any` for the second parameter to `{method_name}`")
    }

    fn autofix_title(&self) -> String {
        format!("Replace with `object`")
    }
}

/// PYI032
pub(crate) fn any_eq_ne_annotation(checker: &mut Checker, name: &str, parameters: &Parameters) {
    if !matches!(name, "__eq__" | "__ne__") {
        return;
    }

    if parameters.args.len() != 2 {
        return;
    }

    let Some(annotation) = &parameters.args[1].parameter.annotation else {
        return;
    };

    if !checker.semantic().current_scope().kind.is_class() {
        return;
    }

    if checker.semantic().match_typing_expr(annotation, "Any") {
        let mut diagnostic = Diagnostic::new(
            AnyEqNeAnnotation {
                method_name: name.to_string(),
            },
            annotation.range(),
        );
        if checker.patch(diagnostic.kind.rule()) {
            // Ex) `def __eq__(self, obj: Any): ...`
            if checker.semantic().is_builtin("object") {
                diagnostic.set_fix(Fix::automatic(Edit::range_replacement(
                    "object".to_string(),
                    annotation.range(),
                )));
            }
        }
        checker.diagnostics.push(diagnostic);
    }
}
