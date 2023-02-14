use rustpython_parser::ast::{Constant, Expr, ExprKind, Stmt};

use rustpython_parser::lexer;
use rustpython_parser::lexer::Tok;

use crate::source_code::Stylist;

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

/// Check if the given piece code is composed of exclusively comments
pub fn code_is_only_comments(code: &str) -> bool {
    let dedented = textwrap::dedent(code);
    let code_tokens = lexer::make_tokenizer(&dedented).flatten();
    for (_, tok, _) in code_tokens {
        if !matches!(
            tok,
            Tok::Comment(..)
                | Tok::String { .. }
                | Tok::Pass
                | Tok::Newline
                | Tok::NonLogicalNewline
        ) {
            return false;
        }
    }
    true
}

/// Extract the indentation from a given line
pub fn extract_indentation(line: &str, stylist: &Stylist) -> String {
    line.chars()
        .take_while(|c| stylist.indentation().contains(*c))
        .collect()
}
