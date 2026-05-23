//! This module takes care of parsing a type annotation.

use ruff_allocator::Allocator;
use ruff_python_ast::relocate::relocate_expr;
use ruff_python_ast::{Expr, ExprStringLiteral, ModExpression, StringLiteral};
use ruff_text_size::Ranged;

use crate::{ParseError, Parsed, parse_expression, parse_string_annotation};

type AnnotationParseResult<'ast> = Result<ParsedAnnotation<'ast>, ParseError>;

#[derive(Debug)]
pub struct ParsedAnnotation<'ast> {
    parsed: Parsed<ModExpression<'ast>>,
    kind: AnnotationKind,
}

impl<'ast> ParsedAnnotation<'ast> {
    pub fn parsed(&self) -> &Parsed<ModExpression<'ast>> {
        &self.parsed
    }

    pub fn expression(&self) -> &Expr<'ast> {
        self.parsed.expr()
    }

    pub fn kind(&self) -> AnnotationKind {
        self.kind
    }
}

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
pub fn parse_type_annotation<'ast>(
    string_expr: &ExprStringLiteral,
    source: &str,
    allocator: &'ast Allocator,
) -> AnnotationParseResult<'ast> {
    if let Some(string_literal) = string_expr.as_single_part_string() {
        // Compare the raw contents (without quotes) of the expression with the parsed contents
        // contained in the string literal.
        if &source[string_literal.content_range()] == string_literal.as_str() {
            parse_simple_type_annotation(string_literal, source, allocator)
        } else {
            // The raw contents of the string doesn't match the parsed content. This could be the
            // case for annotations that contain escaped quotes.
            parse_complex_type_annotation(string_expr, allocator)
        }
    } else {
        // String is implicitly concatenated.
        parse_complex_type_annotation(string_expr, allocator)
    }
}

fn parse_simple_type_annotation<'ast>(
    string_literal: &StringLiteral,
    source: &str,
    allocator: &'ast Allocator,
) -> AnnotationParseResult<'ast> {
    Ok(ParsedAnnotation {
        parsed: parse_string_annotation(source, string_literal, allocator)?,
        kind: AnnotationKind::Simple,
    })
}

fn parse_complex_type_annotation<'ast>(
    string_expr: &ExprStringLiteral,
    allocator: &'ast Allocator,
) -> AnnotationParseResult<'ast> {
    let mut parsed = parse_expression(string_expr.value.to_str(), allocator)?;
    let mut relocated = parsed.expr().clone();
    relocate_expr(&mut relocated, string_expr.range(), allocator);
    parsed.replace_expr(relocated, allocator);
    Ok(ParsedAnnotation {
        parsed,
        kind: AnnotationKind::Complex,
    })
}
