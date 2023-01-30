use libcst_native::{
    Codegen, CodegenState, Expression, ParenthesizableWhitespace, SmallStatement, Statement,
};
use rustpython_ast::{Expr, Keyword, Location};

use crate::ast::types::Range;
use crate::autofix::helpers::remove_argument;
use crate::fix::Fix;
use crate::source_code::{Locator, Stylist};

/// Generate a fix to remove a base from a `ClassDef` statement.
pub fn remove_class_def_base(
    locator: &Locator,
    stmt_at: Location,
    expr_at: Location,
    expr_end: Location,
    bases: &[Expr],
    keywords: &[Keyword],
) -> Option<Fix> {
    if let Ok(fix) = remove_argument(locator, stmt_at, expr_at, expr_end, bases, keywords, true) {
        Some(fix)
    } else {
        None
    }
}

/// Generate a fix to remove arguments from a `super` call.
pub fn remove_super_arguments(locator: &Locator, stylist: &Stylist, expr: &Expr) -> Option<Fix> {
    let range = Range::from_located(expr);
    let contents = locator.slice_source_code_range(&range);

    let mut tree = libcst_native::parse_module(contents, None).ok()?;

    let Statement::Simple(body) = tree.body.first_mut()? else {
        return None;
    };
    let SmallStatement::Expr(body) = body.body.first_mut()? else {
        return None;
    };
    let Expression::Call(body) = &mut body.value else {
        return None;
    };

    body.args = vec![];
    body.whitespace_before_args = ParenthesizableWhitespace::default();
    body.whitespace_after_func = ParenthesizableWhitespace::default();

    let mut state = CodegenState {
        default_newline: stylist.line_ending(),
        default_indent: stylist.indentation(),
        ..CodegenState::default()
    };
    tree.codegen(&mut state);

    Some(Fix::replacement(
        state.to_string(),
        range.location,
        range.end_location,
    ))
}
