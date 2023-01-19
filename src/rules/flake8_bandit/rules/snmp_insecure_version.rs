use num_traits::{One, Zero};
use rustpython_ast::{Expr, ExprKind, Keyword};
use rustpython_parser::ast::Constant;

use crate::ast::helpers::SimpleCallArgs;
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violations;

/// S508
pub fn snmp_insecure_version(
    checker: &mut Checker,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) {
    if checker.resolve_call_path(func).map_or(false, |call_path| {
        call_path.as_slice() == ["pysnmp", "hlapi", "CommunityData"]
    }) {
        let call_args = SimpleCallArgs::new(args, keywords);
        if let Some(mp_model_arg) = call_args.get_argument("mpModel", None) {
            if let ExprKind::Constant {
                value: Constant::Int(value),
                ..
            } = &mp_model_arg.node
            {
                if value.is_zero() || value.is_one() {
                    checker.diagnostics.push(Diagnostic::new(
                        violations::SnmpInsecureVersion,
                        Range::from_located(mp_model_arg),
                    ));
                }
            }
        }
    }
}
