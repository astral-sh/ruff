use rustpython_ast::{Expr, Keyword};

use crate::ast::helpers::SimpleCallArgs;
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violations;

/// S509
pub fn snmp_weak_cryptography(
    checker: &mut Checker,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) {
    if checker.resolve_call_path(func).map_or(false, |call_path| {
        call_path.as_slice() == ["pysnmp", "hlapi", "UsmUserData"]
    }) {
        let call_args = SimpleCallArgs::new(args, keywords);
        if call_args.len() < 3 {
            checker.diagnostics.push(Diagnostic::new(
                violations::SnmpWeakCryptography,
                Range::from_located(func),
            ));
        }
    }
}
