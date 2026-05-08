use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::Stmt;
use ruff_python_ast::token::TokenKind;
use ruff_python_semantic::SemanticModel;
use ruff_source_file::LineRanges;
use ruff_text_size::{TextLen, TextRange, TextSize};

use crate::checkers::ast::Checker;
use crate::{AlwaysFixableViolation, Edit, Fix};

/// ## What it does
/// Checks for the presence of unnecessary quotes in type annotations.
///
/// ## Why is this bad?
/// In Python, type annotations can be quoted to avoid forward references.
///
/// However, if `from __future__ import annotations` is present, Python
/// will always evaluate type annotations in a deferred manner, making
/// the quotes unnecessary.
///
/// Similarly, if the annotation is located in a typing-only context and
/// won't be evaluated by Python at runtime, the quotes will also be
/// considered unnecessary. For example, Python does not evaluate type
/// annotations on assignments in function bodies.
///
/// ## Example
///
/// Given:
///
/// ```python
/// from __future__ import annotations
///
///
/// def foo(bar: "Bar") -> "Bar": ...
/// ```
///
/// Use instead:
///
/// ```python
/// from __future__ import annotations
///
///
/// def foo(bar: Bar) -> Bar: ...
/// ```
///
/// Given:
///
/// ```python
/// def foo() -> None:
///     bar: "Bar"
/// ```
///
/// Use instead:
///
/// ```python
/// def foo() -> None:
///     bar: Bar
/// ```
///
/// ## Preview
///
/// When [preview] is enabled, if [`lint.future-annotations`] is set to `true`,
/// `from __future__ import annotations` will be added if doing so would allow an annotation to be
/// unquoted.
///
/// ## Fix safety
///
/// The rule's fix is marked as unsafe in two cases:
///
/// - When the target version is Python 3.14 or later and the file does not
///   contain `from __future__ import annotations`. Under [PEP 649], Python
///   defers annotation evaluation by default, but tools that introspect
///   annotations eagerly (for example `inspect.signature(..., eval_str=True)`
///   or `unittest.mock.create_autospec`) can still raise `NameError` when an
///   unquoted name is only imported under `if TYPE_CHECKING:`.
/// - When [preview] is enabled, [`lint.future-annotations`] is set to `true`,
///   and a `from __future__ import annotations` import is added. Such an
///   import may change the behavior of all annotations in the file.
///
/// [PEP 649]: https://peps.python.org/pep-0649/
///
/// ## Options
/// - `lint.future-annotations`
///
/// ## See also
/// - [`quoted-annotation-in-stub`][PYI020]: A rule that
///   removes all quoted annotations from stub files
/// - [`quoted-type-alias`][TC008]: A rule that removes unnecessary quotes
///   from type aliases.
///
/// ## References
/// - [PEP 563 – Postponed Evaluation of Annotations](https://peps.python.org/pep-0563/)
/// - [Python documentation: `__future__`](https://docs.python.org/3/library/__future__.html#module-__future__)
///
/// [PYI020]: https://docs.astral.sh/ruff/rules/quoted-annotation-in-stub/
/// [TC008]: https://docs.astral.sh/ruff/rules/quoted-type-alias/
/// [preview]: https://docs.astral.sh/ruff/preview/
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.242")]
pub(crate) struct QuotedAnnotation;

impl AlwaysFixableViolation for QuotedAnnotation {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Remove quotes from type annotation".to_string()
    }

    fn fix_title(&self) -> String {
        "Remove quotes".to_string()
    }
}

/// UP037
pub(crate) fn quoted_annotation(checker: &Checker, annotation: &str, range: TextRange) {
    let add_future_import = checker.settings().future_annotations
        && checker.semantic().in_runtime_evaluated_annotation();

    if !(checker.semantic().in_typing_only_annotation() || add_future_import) {
        return;
    }

    let placeholder_range = TextRange::up_to(annotation.text_len());
    let spans_multiple_lines = annotation.contains_line_break(placeholder_range);

    let last_token_is_comment = checker
        .tokens()
        .iter()
        .rfind(|tok| !tok.kind().is_any_newline())
        .is_some_and(|tok| tok.kind() == TokenKind::Comment);

    let new_content = match (spans_multiple_lines, last_token_is_comment) {
        (_, false) if in_parameter_annotation(range.start(), checker.semantic()) => {
            annotation.to_string()
        }
        (false, false) => annotation.to_string(),
        (true, false) => format!("({annotation})"),
        (_, true) => format!("({annotation}\n)"),
    };
    let unquote_edit = Edit::range_replacement(new_content, range);

    // On Python 3.14+, annotations are lazily evaluated (PEP 649), but tools
    // that introspect annotations eagerly (e.g. `inspect.signature` with
    // `eval_str=True`, or `unittest.mock.create_autospec`) can still raise
    // `NameError` when an unquoted name is only imported under `TYPE_CHECKING`.
    // `from __future__ import annotations` keeps the annotation as a string,
    // so the eager-resolution case can't fire there.
    // See https://github.com/astral-sh/ruff/issues/20782.
    let pep_649_unsafe = checker.target_version().defers_annotations()
        && !checker.semantic().future_annotations_or_stub();

    let fix = if add_future_import {
        let import_edit = checker.importer().add_future_import();
        Fix::unsafe_edits(unquote_edit, [import_edit])
    } else if pep_649_unsafe {
        Fix::unsafe_edit(unquote_edit)
    } else {
        Fix::safe_edit(unquote_edit)
    };

    checker
        .report_diagnostic(QuotedAnnotation, range)
        .set_fix(fix);
}

fn in_parameter_annotation(offset: TextSize, semantic: &SemanticModel) -> bool {
    let Stmt::FunctionDef(stmt) = semantic.current_statement() else {
        return false;
    };

    stmt.parameters.range.contains(offset)
}
