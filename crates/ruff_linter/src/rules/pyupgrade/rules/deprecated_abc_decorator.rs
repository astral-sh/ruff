use ruff_python_ast::Decorator;
use ruff_python_trivia::indentation_at_offset;
use ruff_text_size::Ranged;

use ruff_macros::{ViolationMetadata, derive_message_formats};

use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;
use crate::{AlwaysFixableViolation, Edit, Fix};

/// ## What it does
/// Checks for uses of `abstractclassmethod`, `abstractstaticmethod`, `abstractproperty`.
///
/// ## Why is this bad?
/// These have been deprecated since Python 3.3 and are expected to be removed in a future version
/// of Python.
///
/// ## Example
///
/// ```python
/// import abc
///
///
/// class Foo(abc.ABC):
///     @abc.abstractclassmethod
///     def class_method(cls, arg1): ...
///
///     @abc.abstractstaticmethod
///     def static_method(arg1): ...
///
///     @abc.abstractproperty
///     def prop(self): ...
/// ```
///
/// Use instead:
///
/// ```python
/// import abc
///
///
/// class Foo(abc.ABC):
///     @classmethod
///     @abc.abstractmethod
///     def class_method(cls, arg1): ...
///
///     @staticmethod
///     @abc.abstractmethod
///     def static_method(arg1): ...
///
///     @property
///     @abc.abstractmethod
///     def prop(self): ...
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "0.15.21")]
pub(crate) struct DeprecatedAbcDecorator {
    from: &'static str,
    to: &'static str,
}

impl AlwaysFixableViolation for DeprecatedAbcDecorator {
    #[derive_message_formats]
    fn message(&self) -> String {
        let DeprecatedAbcDecorator { from, to } = self;
        format!("Use `@{to}` and `@abstractmethod` instead of `{from}`")
    }

    fn fix_title(&self) -> String {
        let DeprecatedAbcDecorator { to, .. } = self;
        format!("Replace with `@{to}` and `abstractmethod`")
    }
}

/// UP051
pub(crate) fn deprecated_abc_decorator(checker: &Checker, decorator_list: &[Decorator]) {
    for decorator in decorator_list {
        // Look for, e.g., `import abc; @abc.abstractclassmethod`, etc.
        for (from, to) in [
            ("abstractclassmethod", "classmethod"),
            ("abstractstaticmethod", "staticmethod"),
            ("abstractproperty", "property"),
        ] {
            if checker
                .semantic()
                .resolve_qualified_name(&decorator.expression)
                .is_some_and(|qualified_name| qualified_name.segments() == ["abc", from])
            {
                let mut diagnostic =
                    checker.report_diagnostic(DeprecatedAbcDecorator { from, to }, decorator.range);
                let indentation =
                    indentation_at_offset(decorator.range().start(), checker.source());
                let Some(indentation) = indentation else {
                    continue;
                };
                diagnostic.try_set_fix(|| {
                    let (insert_import_edit, insert_binding) = checker
                        .importer()
                        .get_or_import_builtin_symbol(to, decorator.start(), checker.semantic())?;
                    let insert_edit = Edit::insertion(
                        format!("@{insert_binding}\n{indentation}"),
                        decorator.range().start(),
                    );

                    let (replace_import_edit, replace_binding) =
                        checker.importer().get_or_import_symbol(
                            &ImportRequest::import("abc", "abstractmethod"),
                            decorator.start(),
                            checker.semantic(),
                        )?;
                    let replace_edit =
                        Edit::range_replacement(replace_binding, decorator.expression.range());

                    Ok(Fix::safe_edits(
                        replace_import_edit,
                        insert_import_edit
                            .into_iter()
                            .chain([insert_edit, replace_edit]),
                    ))
                });
            }
        }
    }
}
