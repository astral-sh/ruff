use rustpython_ast::{Expr, ExprKind, Keyword, KeywordData};

use crate::ast::types::Range;
use crate::registry::Diagnostic;
use crate::violations;

mod ambiguous_unicode_character;
mod unpack_instead_of_concatenating_to_collection_literal;

pub use ambiguous_unicode_character::ambiguous_unicode_character;
pub use unpack_instead_of_concatenating_to_collection_literal::unpack_instead_of_concatenating_to_collection_literal;

#[derive(Clone, Copy)]
pub enum Context {
    String,
    Docstring,
    Comment,
}

/// RUF004
pub fn keyword_argument_before_star_argument(
    args: &[Expr],
    keywords: &[Keyword],
) -> Vec<Diagnostic> {
    let mut diagnostics = vec![];
    if let Some(arg) = args
        .iter()
        .rfind(|arg| matches!(arg.node, ExprKind::Starred { .. }))
    {
        for keyword in keywords {
            if keyword.location < arg.location {
                let KeywordData { arg, .. } = &keyword.node;
                if let Some(arg) = arg {
                    diagnostics.push(Diagnostic::new(
                        violations::KeywordArgumentBeforeStarArgument(arg.to_string()),
                        Range::from_located(keyword),
                    ));
                }
            }
        }
    }
    diagnostics
}
