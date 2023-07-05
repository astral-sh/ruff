use rustpython_parser::ast::{Keyword, Ranged};

use crate::checkers::ast::Checker;
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use super::super::helpers::{matches_password_name, string_literal};

#[violation]
pub struct HardcodedPasswordFuncArg {
    name: String,
}

impl Violation for HardcodedPasswordFuncArg {
    #[derive_message_formats]
    fn message(&self) -> String {
        let HardcodedPasswordFuncArg { name } = self;
        format!(
            "Possible hardcoded password assigned to argument: \"{}\"",
            name.escape_debug()
        )
    }
}

/// S106
pub(crate) fn hardcoded_password_func_arg(checker: &mut Checker, keywords: &[Keyword]) {
    checker
        .diagnostics
        .extend(keywords.iter().filter_map(|keyword| {
            string_literal(&keyword.value).filter(|string| !string.is_empty())?;
            let arg = keyword.arg.as_ref()?;
            if !matches_password_name(arg) {
                return None;
            }
            Some(Diagnostic::new(
                HardcodedPasswordFuncArg {
                    name: arg.to_string(),
                },
                keyword.range(),
            ))
        }));
}
