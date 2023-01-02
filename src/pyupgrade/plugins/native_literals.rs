use rustpython_ast::{Constant, Expr, ExprKind, Keyword};
use rustpython_parser::lexer;
use rustpython_parser::lexer::Tok;

use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::registry::{Check, CheckCode, CheckKind, LiteralType};

/// UP018
pub fn native_literals(
    checker: &mut Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) {
    let ExprKind::Name { id, .. } = &func.node else { return; };

    if (id == "str" || id == "bytes")
        && keywords.is_empty()
        && args.len() <= 1
        && checker.is_builtin(id)
    {
        let Some(arg) = args.get(0) else {
            let literal_type = if id == "str" {
                LiteralType::Str
            } else {
                LiteralType::Bytes
            };
             let mut check = Check::new(CheckKind::NativeLiterals(literal_type), Range::from_located(expr));
            if checker.patch(&CheckCode::UP018) {
                check.amend(Fix::replacement(
                    format!("{}\"\"", if id == "bytes" { "b" } else { "" }),
                    expr.location,
                    expr.end_location.unwrap(),
                ));
            }
            checker.add_check(check);
            return;
        };

        let ExprKind::Constant { value, ..} = &arg.node else {
            return;
        };
        let literal_type = match value {
            Constant::Str { .. } => LiteralType::Str,
            Constant::Bytes { .. } => LiteralType::Bytes,
            _ => return,
        };

        // rust-python merges adjacent string/bytes literals into one node, but we can't
        // safely remove the outer call in this situation. We're following pyupgrade
        // here and skip.
        let arg_code = checker
            .locator
            .slice_source_code_range(&Range::from_located(arg));
        if lexer::make_tokenizer(&arg_code)
            .flatten()
            .filter(|(_, tok, _)| matches!(tok, Tok::String { .. }))
            .count()
            > 1
        {
            return;
        }

        let mut check = Check::new(
            CheckKind::NativeLiterals(literal_type),
            Range::from_located(expr),
        );
        if checker.patch(&CheckCode::UP018) {
            check.amend(Fix::replacement(
                arg_code.to_string(),
                expr.location,
                expr.end_location.unwrap(),
            ));
        }
        checker.add_check(check);
    }
}
