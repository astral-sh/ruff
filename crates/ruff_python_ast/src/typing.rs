use anyhow::Result;
use ruff_text_size::{TextLen, TextRange};
use rustpython_parser::ast::Expr;
use rustpython_parser::Parse;

use crate::relocate::relocate_expr;
use crate::source_code::Locator;
use crate::str;

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
    locator: &Locator,
) -> Result<(Expr, AnnotationKind)> {
    let expression = &locator.contents()[range];

    if str::raw_contents(expression).map_or(false, |body| body == value) {
        // The annotation is considered "simple" if and only if the raw representation (e.g.,
        // `List[int]` within "List[int]") exactly matches the parsed representation. This
        // isn't the case, e.g., for implicit concatenations, or for annotations that contain
        // escaped quotes.
        let leading_quote = str::leading_quote(expression).unwrap();
        let expr = Expr::parse_starts_at(
            value,
            "<filename>",
            range.start() + leading_quote.text_len(),
        )?;
        Ok((expr, AnnotationKind::Simple))
    } else {
        // Otherwise, consider this a "complex" annotation.
        let mut expr = Expr::parse(value, "<filename>")?;
        relocate_expr(&mut expr, range);
        Ok((expr, AnnotationKind::Complex))
    }
}
