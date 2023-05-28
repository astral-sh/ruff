use rustpython_parser::ast::Arguments;

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

/// ## What it does
/// Checks for `Any` type annotations for the second parameter in `__ne__` and `__eq__` methods
///
/// ## Why is this bad?
/// From the Python docs: `Use object to indicate that a value could be any type in a typesafe
/// manner. Use Any to indicate that a value is dynamically typed.` For `__eq__` and `__ne__` method
/// we want to do the former.
///
/// ## Example
/// ```python
/// class Foo:
///     def __eq__(self, obj: Any):
///     def __ne__(self, obj: typing.Any):
/// ```
///
/// Use instead:
/// ```python
/// class Foo:
///     def __eq__(self, obj: object):
///     def __ne__(self, obj: object):
/// ```
/// ## References
/// - [Python Docs](https://docs.python.org/3/library/typing.html#the-any-type)
/// - [Mypy Docs](https://mypy.readthedocs.io/en/latest/dynamic_typing.html#any-vs-object)
#[violation]
pub struct AnyEqNeAnnotation {
    method_name: String,
}

impl AlwaysAutofixableViolation for AnyEqNeAnnotation {
    #[derive_message_formats]
    fn message(&self) -> String {
        let AnyEqNeAnnotation { method_name } = self;
        format!("Prefer `object` to `Any` for the second parameter in {method_name}")
    }

    fn autofix_title(&self) -> String {
        format!("Replace `object` with `Any`")
    }
}

/// PYI032
pub(crate) fn any_eq_ne_annotation(checker: &mut Checker, name: &str, args: &Arguments) {
    if !checker.semantic_model().scope().kind.is_class() {
        return;
    }

    // Ignore non `__eq__` and non `__ne__` methods
    if name != "__eq__" && name != "__ne__" {
        return;
    }

    // Different numbers of arguments than 2 are handled by other rules
    if args.args.len() == 2 {
        if let Some(annotation) = &args.args[1].annotation {
            if checker
                .semantic_model()
                .match_typing_expr(annotation, "Any")
            {
                let mut diagnostic = Diagnostic::new(
                    AnyEqNeAnnotation {
                        method_name: name.to_string(),
                    },
                    args.args[1].range,
                );
                if checker.patch(diagnostic.kind.rule()) {
                    // def __eq__(self, arg2: Any): ...
                    if let Some(name) = annotation.as_name_expr() {
                        diagnostic.set_fix(Fix::automatic(Edit::range_replacement(
                            format!("object"),
                            name.range,
                        )));
                    }
                    // def __eq__(self, arg2: typing.Any): ...
                    if let Some(attr) = annotation.as_attribute_expr() {
                        diagnostic.set_fix(Fix::automatic(Edit::range_replacement(
                            format!("object"),
                            attr.range,
                        )));
                    }
                }
                checker.diagnostics.push(diagnostic);
            }
        }
    }
}
