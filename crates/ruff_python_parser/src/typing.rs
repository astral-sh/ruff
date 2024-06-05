//! This module takes care of parsing a type annotation.

use ruff_python_ast::relocate::relocate_expr;
use ruff_python_ast::str::raw_contents;
use ruff_python_ast::{ExprStringLiteral, ModExpression, StringFlags, StringLiteral};
use ruff_text_size::Ranged;

use crate::{parse_expression, parse_expression_range, ParseError, Parsed};

#[derive(Copy, Clone, Debug)]
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

impl AnnotationKind {
    /// Returns `true` if the annotation kind is simple.
    pub const fn is_simple(self) -> bool {
        matches!(self, AnnotationKind::Simple)
    }
}

/// Parses the given string expression node as a type annotation. The given `source` is the entire
/// source code.
pub fn parse_type_annotation(
    string_expr: &ExprStringLiteral,
    source: &str,
) -> Result<(Parsed<ModExpression>, AnnotationKind), ParseError> {
    let expr_text = &source[string_expr.range()];

    if let [string_literal] = string_expr.value.as_slice() {
        // Compare the raw contents (without quotes) of the expression with the parsed contents
        // contained in the string literal.
        if raw_contents(expr_text)
            .is_some_and(|raw_contents| raw_contents == string_literal.as_str())
        {
            parse_simple_type_annotation(string_literal, source)
        } else {
            // The raw contents of the string doesn't match the parsed content. This could be the
            // case for annotations that contain escaped quotes.
            parse_complex_type_annotation(string_expr)
        }
    } else {
        // String is implicitly concatenated.
        parse_complex_type_annotation(string_expr)
    }
}

fn parse_simple_type_annotation(
    string_literal: &StringLiteral,
    source: &str,
) -> Result<(Parsed<ModExpression>, AnnotationKind), ParseError> {
    Ok((
        parse_expression_range(
            source,
            string_literal
                .range()
                .add_start(string_literal.flags.opener_len())
                .sub_end(string_literal.flags.closer_len()),
        )?,
        AnnotationKind::Simple,
    ))
}

fn parse_complex_type_annotation(
    string_expr: &ExprStringLiteral,
) -> Result<(Parsed<ModExpression>, AnnotationKind), ParseError> {
    let mut parsed = parse_expression(string_expr.value.to_str())?;
    relocate_expr(parsed.expr_mut(), string_expr.range());
    Ok((parsed, AnnotationKind::Complex))
}
