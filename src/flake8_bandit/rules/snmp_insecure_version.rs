use num_traits::{One, Zero};
use rustc_hash::{FxHashMap, FxHashSet};
use rustpython_ast::{Expr, ExprKind, Keyword};
use rustpython_parser::ast::Constant;

use crate::ast::helpers::{collect_call_paths, dealias_call_path, match_call_path, SimpleCallArgs};
use crate::ast::types::Range;
use crate::registry::Diagnostic;
use crate::violations;

/// S508
pub fn snmp_insecure_version(
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
    from_imports: &FxHashMap<&str, FxHashSet<&str>>,
    import_aliases: &FxHashMap<&str, &str>,
) -> Option<Diagnostic> {
    let call_path = dealias_call_path(collect_call_paths(func), import_aliases);

    if match_call_path(&call_path, "pysnmp.hlapi", "CommunityData", from_imports) {
        let call_args = SimpleCallArgs::new(args, keywords);

        if let Some(mp_model_arg) = call_args.get_argument("mpModel", None) {
            if let ExprKind::Constant {
                value: Constant::Int(value),
                ..
            } = &mp_model_arg.node
            {
                if value.is_zero() || value.is_one() {
                    return Some(Diagnostic::new(
                        violations::SnmpInsecureVersion,
                        Range::from_located(mp_model_arg),
                    ));
                }
            }
        }
    }
    None
}
