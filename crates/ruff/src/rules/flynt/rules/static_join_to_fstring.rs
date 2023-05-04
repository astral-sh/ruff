use rustpython_parser::ast::{Expr, ExprKind};

use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::{create_expr, unparse_expr};

use crate::checkers::ast::Checker;
use crate::registry::AsRule;
use crate::rules::flynt::helpers;

#[violation]
pub struct StaticJoinToFString {
    pub expr: String,
    pub fixable: bool,
}

impl Violation for StaticJoinToFString {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let StaticJoinToFString { expr, .. } = self;
        format!("Consider `{expr}` instead of string join")
    }

    fn autofix_title_formatter(&self) -> Option<fn(&Self) -> String> {
        self.fixable
            .then_some(|StaticJoinToFString { expr, .. }| format!("Replace with `{expr}`"))
    }
}

fn is_static_length(elts: &[Expr]) -> bool {
    elts.iter()
        .all(|e| !matches!(e.node, ExprKind::Starred { .. }))
}

fn build_fstring(joiner: &str, joinees: &Vec<Expr>) -> Option<Expr> {
    let mut fstring_elems = Vec::with_capacity(joinees.len() * 2);
    for (i, expr) in joinees.iter().enumerate() {
        if matches!(expr.node, ExprKind::JoinedStr { .. }) {
            // Oops, already an f-string. We don't know how to handle those
            // gracefully right now.
            return None;
        }
        let elem = helpers::to_fstring_elem(expr.clone())?;
        if i != 0 {
            fstring_elems.push(helpers::to_constant_string(joiner));
        }
        fstring_elems.push(elem);
    }
    Some(create_expr(ExprKind::JoinedStr {
        values: fstring_elems,
    }))
}

pub fn static_join_to_fstring(checker: &mut Checker, expr: &Expr, joiner: &str) {
    let ExprKind::Call {
        func: _,
        args,
        keywords,
    } = &expr.node else {
        return;
    };

    if !keywords.is_empty() || args.len() != 1 {
        // If there are kwargs or more than one argument,
        // this is some non-standard string join call.
        return;
    }

    // Get the elements to join; skip e.g. generators, sets, etc.
    let joinees = match &args[0].node {
        ExprKind::List { elts, .. } if is_static_length(elts) => elts,
        ExprKind::Tuple { elts, .. } if is_static_length(elts) => elts,
        _ => return,
    };

    // Try to build the fstring (internally checks whether e.g. the elements are
    // convertible to f-string parts).
    let Some(new_expr) = build_fstring(joiner, joinees) else { return };

    let contents = unparse_expr(&new_expr, checker.stylist);
    let fixable = true; // I'm not sure there is a case where this is not fixable..?

    let mut diagnostic = Diagnostic::new(
        StaticJoinToFString {
            expr: contents.clone(),
            fixable,
        },
        expr.range(),
    );
    if checker.patch(diagnostic.kind.rule()) {
        if fixable {
            diagnostic.set_fix(Edit::range_replacement(contents, expr.range()));
        }
    }
    checker.diagnostics.push(diagnostic);
}
