use rustc_hash::{FxHashMap, FxHashSet};
use rustpython_ast::{Expr, Keyword};

use crate::ast::helpers::{collect_call_paths, dealias_call_path, match_call_path, SimpleCallArgs};
use crate::ast::types::Range;
use crate::registry::Diagnostic;
use crate::violations;

/// S509
pub fn snmp_weak_cryptography(
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
    from_imports: &FxHashMap<&str, FxHashSet<&str>>,
    import_aliases: &FxHashMap<&str, &str>,
) -> Option<Diagnostic> {
    let call_path = dealias_call_path(collect_call_paths(func), import_aliases);

    if match_call_path(&call_path, "pysnmp.hlapi", "UsmUserData", from_imports) {
        let call_args = SimpleCallArgs::new(args, keywords);

        if call_args.len() < 3 {
            return Some(Diagnostic::new(
                violations::SnmpWeakCryptography,
                Range::from_located(func),
            ));
        }
    }
    None
}
