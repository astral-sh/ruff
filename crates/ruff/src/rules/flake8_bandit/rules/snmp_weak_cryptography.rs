use rustpython_parser::ast::{Expr, Keyword, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::SimpleCallArgs;

use crate::checkers::ast::Checker;

#[violation]
pub struct SnmpWeakCryptography;

impl Violation for SnmpWeakCryptography {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "You should not use SNMPv3 without encryption. `noAuthNoPriv` & `authNoPriv` is \
             insecure."
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
        let call_args = SimpleCallArgs::new(args, keywords);
        if call_args.len() < 3 {
            checker
                .diagnostics
                .push(Diagnostic::new(SnmpWeakCryptography, func.range()));
        }
    }
}
