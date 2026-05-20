use ruff_diagnostics::Applicability;
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::Decorator;
use ruff_python_ast::whitespace::indentation;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;
use crate::{Edit, Fix, FixAvailability, Violation};

/// ## What it does
/// Checks for deprecated decorators from the `abc` module.
///
/// ## Why is this bad?
/// `abc.abstractclassmethod`, `abc.abstractstaticmethod`, and
/// `abc.abstractproperty` are deprecated. Instead, use `abc.abstractmethod`
/// with `classmethod`, `staticmethod`, or `property`.
///
/// ## Example
/// ```python
/// import abc
///
///
/// class Foo:
///     @abc.abstractclassmethod
///     def bar(cls): ...
/// ```
///
/// Use instead:
/// ```python
/// import abc
///
///
/// class Foo:
///     @classmethod
///     @abc.abstractmethod
///     def bar(cls): ...
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as safe.
///
/// ## References
/// - [Python documentation: `abc.abstractmethod`](https://docs.python.org/3/library/abc.html#abc.abstractmethod)
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "NEXT_RUFF_VERSION")]
pub(crate) struct DeprecatedAbcDecorator {
    kind: DeprecatedAbcDecoratorKind,
}

impl Violation for DeprecatedAbcDecorator {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let DeprecatedAbcDecorator { kind } = self;
        format!(
            "`abc.{}` is deprecated, use `@{}` with `@abc.abstractmethod`",
            kind.deprecated_name(),
            kind.replacement_name()
        )
    }

    fn fix_title(&self) -> Option<String> {
        let DeprecatedAbcDecorator { kind } = self;
        Some(format!(
            "Rewrite with `@{}` and `@abc.abstractmethod`",
            kind.replacement_name()
        ))
    }
}

#[derive(Debug, Clone, Copy)]
enum DeprecatedAbcDecoratorKind {
    ClassMethod,
    StaticMethod,
    Property,
}

impl DeprecatedAbcDecoratorKind {
    const fn deprecated_name(self) -> &'static str {
        match self {
            Self::ClassMethod => "abstractclassmethod",
            Self::StaticMethod => "abstractstaticmethod",
            Self::Property => "abstractproperty",
        }
    }

    const fn replacement_name(self) -> &'static str {
        match self {
            Self::ClassMethod => "classmethod",
            Self::StaticMethod => "staticmethod",
            Self::Property => "property",
        }
    }
}

/// UP051
pub(crate) fn deprecated_abc_decorator(checker: &Checker, decorator_list: &[Decorator]) {
    for decorator in decorator_list {
        let Some(kind) = checker
            .semantic()
            .resolve_qualified_name(&decorator.expression)
            .and_then(|qualified_name| match qualified_name.segments() {
                ["abc", "abstractclassmethod"] => Some(DeprecatedAbcDecoratorKind::ClassMethod),
                ["abc", "abstractstaticmethod"] => Some(DeprecatedAbcDecoratorKind::StaticMethod),
                ["abc", "abstractproperty"] => Some(DeprecatedAbcDecoratorKind::Property),
                _ => None,
            })
        else {
            continue;
        };

        let mut diagnostic =
            checker.report_diagnostic(DeprecatedAbcDecorator { kind }, decorator.range());
        diagnostic.add_primary_tag(ruff_db::diagnostic::DiagnosticTag::Deprecated);
        diagnostic.try_set_fix(|| {
            let (decorator_import_edit, decorator_binding) =
                checker.importer().get_or_import_builtin_symbol(
                    kind.replacement_name(),
                    decorator.start(),
                    checker.semantic(),
                )?;
            let (abstractmethod_import_edit, abstractmethod_binding) =
                checker.importer().get_or_import_symbol(
                    &ImportRequest::import("abc", "abstractmethod"),
                    decorator.start(),
                    checker.semantic(),
                )?;

            let line_ending = checker.stylist().line_ending().as_str();
            let indent = indentation(checker.locator().contents(), decorator).unwrap_or("");
            let replacement =
                format!("@{decorator_binding}{line_ending}{indent}@{abstractmethod_binding}");

            let replacement_edit = Edit::range_replacement(replacement, decorator.range());

            let mut edits = vec![replacement_edit, abstractmethod_import_edit];
            if let Some(decorator_import_edit) = decorator_import_edit {
                edits.push(decorator_import_edit);
            }

            let first = edits.remove(0);
            Ok(Fix::applicable_edits(first, edits, Applicability::Safe))
        });
    }
}
