use crate::define_violation;
use crate::violation::Violation;
use ruff_macros::derive_message_formats;
use rustpython_ast::Keyword;

use super::super::helpers::{matches_password_name, string_literal};
use crate::ast::types::Range;
use crate::registry::Diagnostic;

define_violation!(
    pub struct HardcodedPasswordFuncArg {
        pub string: String,
    }
);
impl Violation for HardcodedPasswordFuncArg {
    #[derive_message_formats]
    fn message(&self) -> String {
        let HardcodedPasswordFuncArg { string } = self;
        format!("Possible hardcoded password: \"{}\"", string.escape_debug())
    }
}

/// S106
pub fn hardcoded_password_func_arg(keywords: &[Keyword]) -> Vec<Diagnostic> {
    keywords
        .iter()
        .filter_map(|keyword| {
            let string = string_literal(&keyword.node.value).filter(|string| !string.is_empty())?;
            let arg = keyword.node.arg.as_ref()?;
            if !matches_password_name(arg) {
                return None;
            }
            Some(Diagnostic::new(
                HardcodedPasswordFuncArg {
                    string: string.to_string(),
                },
                Range::from_located(keyword),
            ))
        })
        .collect()
}
