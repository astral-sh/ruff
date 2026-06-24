use ruff_python_ast::Keyword;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;

use crate::rules::flake8_bandit::helpers::{matches_password_name, string_literal};

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
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.116")]
pub(crate) struct HardcodedPasswordFuncArg {
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
pub(crate) fn hardcoded_password_func_arg(checker: &Checker, keywords: &[Keyword]) {
    for keyword in keywords {
        if string_literal(&keyword.value).is_none_or(str::is_empty) {
            continue;
        }
        let Some(arg) = &keyword.arg else {
            continue;
        };
        if !matches_password_name(arg) {
            continue;
        }
        checker.report_diagnostic(
            HardcodedPasswordFuncArg {
                name: arg.to_string(),
            },
            keyword.range(),
        );
    }
}
