use ruff_python_ast::Keyword;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

use super::super::helpers::{matches_password_name, string_literal};

/// ## What it does
/// Checks for potential uses of hardcoded passwords in function calls.
///
/// ## Why is this bad?
/// Including a hardcoded password in source code is a security risk, as an
/// attacker could discover the password and use it to gain unauthorized
/// access.
///
/// Instead, store passwords and other secrets in configuration files,
/// environment variables, or other sources that are excluded from version
/// control.
///
/// ## Example
/// ```python
/// connect_to_server(password="hunter2")
/// ```
///
/// Use instead:
/// ```python
/// import os
///
/// connect_to_server(password=os.environ["PASSWORD"])
/// ```
///
/// ## References
/// - [Common Weakness Enumeration: CWE-259](https://cwe.mitre.org/data/definitions/259.html)
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
