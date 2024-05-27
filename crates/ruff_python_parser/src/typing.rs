//! This module takes care of parsing a type annotation.

use anyhow::Result;

use ruff_python_ast::relocate::relocate_expr;
use ruff_python_ast::{str, Expr};
use ruff_text_size::{TextLen, TextRange};

use crate::{parse_expression, parse_expression_range};

#[derive(is_macro::Is, Copy, Clone, Debug)]
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

/// Parses the value of a string literal node (`parsed_contents`) with `range` as a type
/// annotation. The given `source` is the entire source code.
pub fn parse_type_annotation(
    parsed_contents: &str,
    range: TextRange,
    source: &str,
) -> Result<(Expr, AnnotationKind)> {
    let expression = &source[range];

    if str::raw_contents(expression).is_some_and(|raw_contents| raw_contents == parsed_contents) {
        // The annotation is considered "simple" if and only if the raw representation (e.g.,
        // `List[int]` within "List[int]") exactly matches the parsed representation. This
        // isn't the case, e.g., for implicit concatenations, or for annotations that contain
        // escaped quotes.
        let leading_quote_len = str::leading_quote(expression).unwrap().text_len();
        let trailing_quote_len = str::trailing_quote(expression).unwrap().text_len();
        let range = range
            .add_start(leading_quote_len)
            .sub_end(trailing_quote_len);
        let expr = parse_expression_range(source, range)?.into_expr();
        Ok((expr, AnnotationKind::Simple))
    } else {
        // Otherwise, consider this a "complex" annotation.
        let mut expr = parse_expression(parsed_contents)?.into_expr();
        relocate_expr(&mut expr, range);
        Ok((expr, AnnotationKind::Complex))
    }
}
