use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, StringLike};
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for hardcoded bindings to all network interfaces (`0.0.0.0`).
///
/// ## Why is this bad?
/// Binding to all network interfaces is insecure as it allows access from
/// unintended interfaces, which may be poorly secured or unauthorized.
///
/// Instead, bind to specific interfaces.
///
/// ## Example
/// ```python
/// ALLOWED_HOSTS = ["0.0.0.0"]
/// ```
///
/// Use instead:
/// ```python
/// ALLOWED_HOSTS = ["127.0.0.1", "localhost"]
/// ```
///
/// ## References
/// - [Common Weakness Enumeration: CWE-200](https://cwe.mitre.org/data/definitions/200.html)
#[derive(ViolationMetadata)]
pub(crate) struct HardcodedBindAllInterfaces;

impl Violation for HardcodedBindAllInterfaces {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Possible binding to all interfaces".to_string()
    }
}

/// S104
pub(crate) fn hardcoded_bind_all_interfaces(checker: &Checker, string: StringLike) {
    match string {
        StringLike::String(ast::ExprStringLiteral { value, .. }) => {
            if value == "0.0.0.0" {
                checker.report_diagnostic(HardcodedBindAllInterfaces, string.range());
            }
        }
        StringLike::FString(ast::ExprFString { value, .. }) => {
            for part in value {
                match part {
                    ast::FStringPart::Literal(literal) => {
                        if &**literal == "0.0.0.0" {
                            checker.report_diagnostic(HardcodedBindAllInterfaces, literal.range());
                        }
                    }
                    ast::FStringPart::FString(f_string) => {
                        for literal in f_string.elements.literals() {
                            if &**literal == "0.0.0.0" {
                                checker
                                    .report_diagnostic(HardcodedBindAllInterfaces, literal.range());
                            }
                        }
                    }
                }
            }
        }

        StringLike::Bytes(_) => (),
        // TODO(dylan): decide whether to trigger here
        StringLike::TString(_) => (),
    }
}
