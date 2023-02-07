use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Expr, ExprKind, Keyword, KeywordData};

use crate::ast::types::Range;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    pub struct KeywordArgumentBeforeStarArgument {
        pub name: String,
    }
);
impl Violation for KeywordArgumentBeforeStarArgument {
    #[derive_message_formats]
    fn message(&self) -> String {
        let KeywordArgumentBeforeStarArgument { name } = self;
        format!("Keyword argument `{name}` must come after starred arguments")
    }
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
                        KeywordArgumentBeforeStarArgument {
                            name: arg.to_string(),
                        },
                        Range::from_located(keyword),
                    ));
                }
            }
        }
    }
    diagnostics
}
