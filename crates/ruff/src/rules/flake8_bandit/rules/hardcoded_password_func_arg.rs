use rustpython_parser::ast::{Keyword, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use super::super::helpers::{matches_password_name, string_literal};

/// ## What it does
/// Checks for hardcoded password arguments.
///
/// ## Why is this bad?
/// Hardcoded passwords are a security risk because they can be easily
/// discovered by attackers and used to gain unauthorized access. As they are
/// hardcoded, this vulnerability cannot be easily fixed without changing the
/// source code.
///
/// Instead of hardcoding passwords, consider storing them in configuration
/// files or other stores that are not committed to version control.
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
pub(crate) fn hardcoded_password_func_arg(keywords: &[Keyword]) -> Vec<Diagnostic> {
    keywords
        .iter()
        .filter_map(|keyword| {
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
        })
        .collect()
}
