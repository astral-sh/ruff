use itertools::Itertools;
use ruff_text_size::TextRange;
use rustpython_parser::ast::{self, Constant, Expr, Ranged};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;
use crate::registry::AsRule;
use crate::rules::flynt::helpers;

#[violation]
pub struct StaticJoinToFString {
    expr: String,
}

impl AlwaysAutofixableViolation for StaticJoinToFString {
    #[derive_message_formats]
    fn message(&self) -> String {
        let StaticJoinToFString { expr } = self;
        format!("Consider `{expr}` instead of string join")
    }

    fn autofix_title(&self) -> String {
        let StaticJoinToFString { expr } = self;
        format!("Replace with `{expr}`")
    }
}

fn is_static_length(elts: &[Expr]) -> bool {
    elts.iter().all(|e| !matches!(e, Expr::Starred(_)))
}

fn build_fstring(joiner: &str, joinees: &[Expr]) -> Option<Expr> {
    let mut fstring_elems = Vec::with_capacity(joinees.len() * 2);
    let mut first = true;
    let mut strings: Vec<&String> = vec![];

    for expr in joinees {
        if matches!(expr, Expr::JoinedStr(_)) {
            // Oops, already an f-string. We don't know how to handle those
            // gracefully right now.
            return None;
        }
        if !std::mem::take(&mut first) {
            fstring_elems.push(helpers::to_constant_string(joiner));
        }
        fstring_elems.push(helpers::to_fstring_elem(expr)?);

        if let Expr::Constant(ast::ExprConstant {
            value: Constant::Str(value),
            ..
        }) = expr
        {
            strings.push(value);
        }
    }

    let node = if strings.len() == joinees.len() {
        ast::Expr::Constant(ast::ExprConstant {
            value: Constant::Str(strings.iter().join(joiner)),
            range: TextRange::default(),
            kind: None,
        })
    } else {
        ast::ExprJoinedStr {
            values: fstring_elems,
            range: TextRange::default(),
        }
        .into()
    };

    Some(node)
}

pub(crate) fn static_join_to_fstring(checker: &mut Checker, expr: &Expr, joiner: &str) {
    let Expr::Call(ast::ExprCall {
        args,
        keywords,
        ..
    }) = expr else {
        return;
    };

    if !keywords.is_empty() || args.len() != 1 {
        // If there are kwargs or more than one argument, this is some non-standard
        // string join call.
        return;
    }

    // Get the elements to join; skip (e.g.) generators, sets, etc.
    let joinees = match &args[0] {
        Expr::List(ast::ExprList { elts, .. }) if is_static_length(elts) => elts,
        Expr::Tuple(ast::ExprTuple { elts, .. }) if is_static_length(elts) => elts,
        _ => return,
    };

    // Try to build the fstring (internally checks whether e.g. the elements are
    // convertible to f-string parts).
    let Some(new_expr) = build_fstring(joiner, joinees) else { return };

    let contents = checker.generator().expr(&new_expr);

    let mut diagnostic = Diagnostic::new(
        StaticJoinToFString {
            expr: contents.clone(),
        },
        expr.range(),
    );
    if checker.patch(diagnostic.kind.rule()) {
        #[allow(deprecated)]
        diagnostic.set_fix(Fix::unspecified(Edit::range_replacement(
            contents,
            expr.range(),
        )));
    }
    checker.diagnostics.push(diagnostic);
}
