use rustpython_ast::{Constant, Expr, ExprKind, Keyword};
use rustpython_parser::lexer;
use rustpython_parser::lexer::Tok;

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::{Diagnostic, Rule};
use crate::violations;
use crate::violations::LiteralType;

/// UP018
pub fn native_literals(
    checker: &mut Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) {
    let ExprKind::Name { id, .. } = &func.node else { return; };

    if !keywords.is_empty() || args.len() > 1 {
        return;
    }

    if (id == "str" || id == "bytes") && checker.is_builtin(id) {
        let Some(arg) = args.get(0) else {
            let mut diagnostic = Diagnostic::new(violations::NativeLiterals(if id == "str" {
                LiteralType::Str
            } else {
                LiteralType::Bytes
            }), Range::from_located(expr));
            if checker.patch(&Rule::NativeLiterals) {
                diagnostic.amend(Fix::replacement(
                    if id == "bytes" {
                        let mut content = String::with_capacity(3);
                        content.push('b');
                        content.push(checker.stylist.quote().into());
                        content.push(checker.stylist.quote().into());
                        content
                    } else {
                        let mut content = String::with_capacity(2);
                        content.push(checker.stylist.quote().into());
                        content.push(checker.stylist.quote().into());
                        content
                    },
                    expr.location,
                    expr.end_location.unwrap(),
                ));
            }
            checker.diagnostics.push(diagnostic);
            return;
        };

        // Look for `str("")`.
        if id == "str"
            && !matches!(
                &arg.node,
                ExprKind::Constant {
                    value: Constant::Str(_),
                    ..
                },
            )
        {
            return;
        }

        // Look for `bytes(b"")`
        if id == "bytes"
            && !matches!(
                &arg.node,
                ExprKind::Constant {
                    value: Constant::Bytes(_),
                    ..
                },
            )
        {
            return;
        }

        // rust-python merges adjacent string/bytes literals into one node, but we can't
        // safely remove the outer call in this situation. We're following pyupgrade
        // here and skip.
        let arg_code = checker
            .locator
            .slice_source_code_range(&Range::from_located(arg));
        if lexer::make_tokenizer(arg_code)
            .flatten()
            .filter(|(_, tok, _)| matches!(tok, Tok::String { .. }))
            .count()
            > 1
        {
            return;
        }

        let mut diagnostic = Diagnostic::new(
            violations::NativeLiterals(if id == "str" {
                LiteralType::Str
            } else {
                LiteralType::Bytes
            }),
            Range::from_located(expr),
        );
        if checker.patch(&Rule::NativeLiterals) {
            diagnostic.amend(Fix::replacement(
                arg_code.to_string(),
                expr.location,
                expr.end_location.unwrap(),
            ));
        }
        checker.diagnostics.push(diagnostic);
    }
}
