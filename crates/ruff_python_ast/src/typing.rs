use anyhow::Result;
use rustpython_parser as parser;
use rustpython_parser::ast::{Expr, ExprKind, Location};

use ruff_python_stdlib::typing::{PEP_585_BUILTINS_ELIGIBLE, PEP_593_SUBSCRIPTS, SUBSCRIPTS};

use crate::context::Context;
use crate::relocate::relocate_expr;
use crate::source_code::Locator;
use crate::str;
use crate::types::Range;

pub enum Callable {
    ForwardRef,
    Cast,
    NewType,
    TypeVar,
    NamedTuple,
    TypedDict,
    MypyExtension,
}

pub enum SubscriptKind {
    AnnotatedSubscript,
    PEP593AnnotatedSubscript,
}

pub fn match_annotated_subscript<'a>(
    expr: &Expr,
    context: &Context,
    typing_modules: impl Iterator<Item = &'a str>,
) -> Option<SubscriptKind> {
    if !matches!(
        expr.node,
        ExprKind::Name { .. } | ExprKind::Attribute { .. }
    ) {
        return None;
    }

    context.resolve_call_path(expr).and_then(|call_path| {
        if SUBSCRIPTS.contains(&call_path.as_slice()) {
            return Some(SubscriptKind::AnnotatedSubscript);
        }
        if PEP_593_SUBSCRIPTS.contains(&call_path.as_slice()) {
            return Some(SubscriptKind::PEP593AnnotatedSubscript);
        }

        for module in typing_modules {
            let module_call_path = module.split('.').collect::<Vec<_>>();
            if call_path.starts_with(&module_call_path) {
                for subscript in SUBSCRIPTS.iter() {
                    if call_path.last() == subscript.last() {
                        return Some(SubscriptKind::AnnotatedSubscript);
                    }
                }
                for subscript in PEP_593_SUBSCRIPTS.iter() {
                    if call_path.last() == subscript.last() {
                        return Some(SubscriptKind::PEP593AnnotatedSubscript);
                    }
                }
            }
        }

        None
    })
}

/// Returns `true` if `Expr` represents a reference to a typing object with a
/// PEP 585 built-in.
pub fn is_pep585_builtin(expr: &Expr, context: &Context) -> bool {
    context.resolve_call_path(expr).map_or(false, |call_path| {
        PEP_585_BUILTINS_ELIGIBLE.contains(&call_path.as_slice())
    })
}

#[derive(is_macro::Is)]
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
    range: Range,
    locator: &Locator,
) -> Result<(Expr, AnnotationKind)> {
    let expression = locator.slice(range);
    let body = str::raw_contents(expression);
    if body == value {
        // The annotation is considered "simple" if and only if the raw representation (e.g.,
        // `List[int]` within "List[int]") exactly matches the parsed representation. This
        // isn't the case, e.g., for implicit concatenations, or for annotations that contain
        // escaped quotes.
        let leading_quote = str::leading_quote(expression).unwrap();
        let expr = parser::parse_expression_located(
            body,
            "<filename>",
            Location::new(
                range.location.row(),
                range.location.column() + leading_quote.len(),
            ),
        )?;
        Ok((expr, AnnotationKind::Simple))
    } else {
        // Otherwise, consider this a "complex" annotation.
        let mut expr = parser::parse_expression(value, "<filename>")?;
        relocate_expr(&mut expr, range);
        Ok((expr, AnnotationKind::Complex))
    }
}
