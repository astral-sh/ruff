use rustpython_ast::{Constant, Expr, ExprKind, Stmt};

use rustpython_parser::lexer;
use rustpython_parser::lexer::Tok;

/// Return `true` if a function's return statement include at least one
/// non-`None` value.
pub fn result_exists(returns: &[(&Stmt, Option<&Expr>)]) -> bool {
    returns.iter().any(|(_, expr)| {
        expr.map(|expr| {
            !matches!(
                expr.node,
                ExprKind::Constant {
                    value: Constant::None,
                    ..
                }
            )
        })
        .unwrap_or(false)
    })
}

/// Check if the specified range is composed of exclusively comments
/// and a single return statement.
pub fn code_is_only_comments(code: &str) -> bool {
    let code_tokens = lexer::make_tokenizer(code).flatten();
    for (_, tok, _) in code_tokens {
        if !matches!(
            tok,
            Tok::Comment(..)
                | Tok::String { .. }
                | Tok::Pass
                | Tok::Newline
                | Tok::NonLogicalNewline
        ) {
            println!("{:?}", tok);
            return false;
        }
    }
    println!("{:?}", code);
    true
}
