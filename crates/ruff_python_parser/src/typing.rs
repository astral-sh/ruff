use crate::{parse_expression, parse_expression_starts_at};
use anyhow::Result;
use ruff_python_ast::relocate::relocate_expr;
use ruff_python_ast::str;
use ruff_python_ast::Expr;
use ruff_text_size::{TextLen, TextRange};

#[derive(is_macro::Is, Copy, Clone)]
pub enum AnnotationKind {
    /// The annotation is defined as part a simple string literal,
    /// e.g. `x: "List[int]" = []`. Annotations within simple literals
    /// can be accurately located. For example, we can underline specific
    /// expressions within the annotation and apply automatic fixes, which is
    /// not possible for complex string literals.
    Simple,
    /// The annotation is defined as part of a complex string literal, such as
    /// a literal containing an implicit concatenation or escaped characters,
    /// e.g. `x: "List" "[int]" = []`. These are comparatively rare, but valid.
    Complex,
}

/// Parse a type annotation from a string.
pub fn parse_type_annotation(
    value: &str,
    range: TextRange,
    source: &str,
) -> Result<(Expr, AnnotationKind)> {
    let expression = &source[range];

    if str::raw_contents(expression).is_some_and(|body| body == value) {
        // The annotation is considered "simple" if and only if the raw representation (e.g.,
        // `List[int]` within "List[int]") exactly matches the parsed representation. This
        // isn't the case, e.g., for implicit concatenations, or for annotations that contain
        // escaped quotes.
        let leading_quote = str::leading_quote(expression).unwrap();
        let expr = parse_expression_starts_at(
            value,
            "<filename>",
            range.start() + leading_quote.text_len(),
        )?;
        Ok((expr, AnnotationKind::Simple))
    } else {
        // Otherwise, consider this a "complex" annotation.
        let mut expr = parse_expression(value, "<filename>")?;
        relocate_expr(&mut expr, range);
        Ok((expr, AnnotationKind::Complex))
    }
}
