use ruff_python_ast::{Expr, Keyword, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::CallArguments;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for uses of the SNMPv3 protocol without encryption.
///
/// ## Why is this bad?
/// Unencrypted SNMPv3 communication can be intercepted and read by
/// unauthorized parties. Instead, enable encryption when using SNMPv3.
///
/// ## Example
/// ```python
/// from pysnmp.hlapi import UsmUserData
///
/// UsmUserData("user")
/// ```
///
/// Use instead:
/// ```python
/// from pysnmp.hlapi import UsmUserData
///
/// UsmUserData("user", "authkey", "privkey")
/// ```
///
/// ## References
/// - [Common Weakness Enumeration: CWE-319](https://cwe.mitre.org/data/definitions/319.html)
#[violation]
pub struct SnmpWeakCryptography;

impl Violation for SnmpWeakCryptography {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "You should not use SNMPv3 without encryption. `noAuthNoPriv` & `authNoPriv` is insecure."
        )
    }
}

/// S509
pub(crate) fn snmp_weak_cryptography(
    checker: &mut Checker,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) {
    if checker
        .semantic()
        .resolve_call_path(func)
        .map_or(false, |call_path| {
            matches!(call_path.as_slice(), ["pysnmp", "hlapi", "UsmUserData"])
        })
    {
        if CallArguments::new(args, keywords).len() < 3 {
            checker
                .diagnostics
                .push(Diagnostic::new(SnmpWeakCryptography, func.range()));
        }
    }
}
