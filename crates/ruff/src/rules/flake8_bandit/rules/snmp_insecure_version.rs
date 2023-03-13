use num_traits::{One, Zero};
use rustpython_parser::ast::{Constant, Expr, ExprKind, Keyword};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::SimpleCallArgs;
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;

#[violation]
pub struct SnmpInsecureVersion;

impl Violation for SnmpInsecureVersion {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("The use of SNMPv1 and SNMPv2 is insecure. Use SNMPv3 if able.")
    }
}

/// S508
pub fn snmp_insecure_version(
    checker: &mut Checker,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) {
    if checker
        .ctx
        .resolve_call_path(func)
        .map_or(false, |call_path| {
            call_path.as_slice() == ["pysnmp", "hlapi", "CommunityData"]
        })
    {
        let call_args = SimpleCallArgs::new(args, keywords);
        if let Some(mp_model_arg) = call_args.keyword_argument("mpModel") {
            if let ExprKind::Constant {
                value: Constant::Int(value),
                ..
            } = &mp_model_arg.node
            {
                if value.is_zero() || value.is_one() {
                    checker.diagnostics.push(Diagnostic::new(
                        SnmpInsecureVersion,
                        Range::from(mp_model_arg),
                    ));
                }
            }
        }
    }
}
