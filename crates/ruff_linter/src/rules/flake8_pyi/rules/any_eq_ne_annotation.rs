use ruff_python_ast::Parameters;

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `__eq__` and `__ne__` implementations that use `typing.Any` as
/// the type annotation for their second parameter.
///
/// ## Why is this bad?
/// The Python documentation recommends the use of `object` to "indicate that a
/// value could be any type in a typesafe manner". `Any`, on the other hand,
/// should be seen as an "escape hatch when you need to mix dynamically and
/// statically typed code". Since using `Any` allows you to write highly unsafe
/// code, you should generally only use `Any` when the semantics of your code
/// would otherwise be inexpressible to the type checker.
///
/// The expectation in Python is that a comparison of two arbitrary objects
/// using `==` or `!=` should never raise an exception. This contract can be
/// fully expressed in the type system and does not involve requesting unsound
/// behaviour from a type checker. As such, `object` is a more appropriate
/// annotation than `Any` for the second parameter of the methods implementing
/// these comparison operators -- `__eq__` and `__ne__`.
///
/// ## Example
///
/// ```pyi
/// class Foo:
///     def __eq__(self, obj: typing.Any) -> bool: ...
/// ```
///
/// Use instead:
///
/// ```pyi
/// class Foo:
///     def __eq__(self, obj: object) -> bool: ...
/// ```
/// ## References
/// - [Python documentation: The `Any` type](https://docs.python.org/3/library/typing.html#the-any-type)
/// - [Mypy documentation: Any vs. object](https://mypy.readthedocs.io/en/latest/dynamic_typing.html#any-vs-object)
#[violation]
pub struct AnyEqNeAnnotation {
    method_name: String,
}

impl AlwaysFixableViolation for AnyEqNeAnnotation {
    #[derive_message_formats]
    fn message(&self) -> String {
        let AnyEqNeAnnotation { method_name } = self;
        format!("Prefer `object` to `Any` for the second parameter to `{method_name}`")
    }

    fn fix_title(&self) -> String {
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

    let semantic = checker.semantic();

    if !semantic.current_scope().kind.is_class() {
        return;
    }

    if !checker.match_maybe_stringized_annotation(annotation, |expr| {
        semantic.match_typing_expr(expr, "Any")
    }) {
        return;
    }

    let mut diagnostic = Diagnostic::new(
        AnyEqNeAnnotation {
            method_name: name.to_string(),
        },
        annotation.range(),
    );
    // Ex) `def __eq__(self, obj: Any): ...`
    diagnostic.try_set_fix(|| {
        let (import_edit, binding) = checker.importer().get_or_import_builtin_symbol(
            "object",
            annotation.start(),
            semantic,
        )?;
        let binding_edit = Edit::range_replacement(binding, annotation.range());
        Ok(Fix::safe_edits(binding_edit, import_edit))
    });
    checker.diagnostics.push(diagnostic);
}
