use ruff_python_parser::TokenKind;
use ruff_text_size::{TextLen, TextRange, TextSize};

use crate::checkers::ast::Checker;
use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::Stmt;
use ruff_python_semantic::SemanticModel;
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
pub(crate) fn quoted_annotation(checker: &Checker, annotation: &str, range: TextRange) {
    let diagnostic = Diagnostic::new(QuotedAnnotation, range);

    let placeholder_range = TextRange::up_to(annotation.text_len());
    let spans_multiple_lines = annotation.contains_line_break(placeholder_range);

    let last_token_is_comment = checker
        .tokens()
        // The actual last token will always be a logical newline,
        // so we check the second to last
        .get(checker.tokens().len().saturating_sub(2))
        .is_some_and(|tok| tok.kind() == TokenKind::Comment);

    let new_content = match (spans_multiple_lines, last_token_is_comment) {
        (_, false) if in_parameter_annotation(range.start(), checker.semantic()) => {
            annotation.to_string()
        }
        (false, false) => annotation.to_string(),
        (true, false) => format!("({annotation})"),
        (_, true) => format!("({annotation}\n)"),
    };
    let edit = Edit::range_replacement(new_content, range);
    let fix = Fix::safe_edit(edit);

    checker.report_diagnostic(diagnostic.with_fix(fix));
}

fn in_parameter_annotation(offset: TextSize, semantic: &SemanticModel) -> bool {
    let Stmt::FunctionDef(stmt) = semantic.current_statement() else {
        return false;
    };

    stmt.parameters.range.contains(offset)
}
