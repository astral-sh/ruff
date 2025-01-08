use ruff_text_size::{TextRange, TextSize};

use crate::checkers::ast::Checker;
use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_trivia::{SimpleToken, SimpleTokenKind, SimpleTokenizer};
use ruff_source_file::LineRanges;

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
/// ## References
/// - [PEP 563 – Postponed Evaluation of Annotations](https://peps.python.org/pep-0563/)
/// - [Python documentation: `__future__`](https://docs.python.org/3/library/__future__.html#module-__future__)
#[derive(ViolationMetadata)]
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
pub(crate) fn quoted_annotation(checker: &mut Checker, annotation: &str, range: TextRange) {
    let diagnostic = Diagnostic::new(QuotedAnnotation, range);

    let len = TextSize::try_from(annotation.len()).unwrap();
    let placeholder_range = TextRange::up_to(len);
    let spans_multiple_lines = annotation.count_lines(placeholder_range) > 1;

    let tokenizer = SimpleTokenizer::new(annotation, placeholder_range);
    let last_token_is_comment = matches!(
        tokenizer.last(),
        Some(SimpleToken {
            kind: SimpleTokenKind::Comment,
            ..
        })
    );

    let new_content = match (spans_multiple_lines, last_token_is_comment) {
        (false, false) => annotation.to_string(),
        (true, false) => format!("({annotation})"),
        (_, true) => format!("({annotation}\n)"),
    };
    let edit = Edit::range_replacement(new_content, range);
    let fix = Fix::safe_edit(edit);

    checker.diagnostics.push(diagnostic.with_fix(fix));
}
