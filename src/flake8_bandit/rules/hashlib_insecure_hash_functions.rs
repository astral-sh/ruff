use rustc_hash::{FxHashMap, FxHashSet};
use rustpython_ast::{Constant, Expr, ExprKind, Keyword};

use crate::ast::helpers::{match_module_member, SimpleCallArgs};
use crate::ast::types::Range;
use crate::flake8_bandit::helpers::string_literal;
use crate::registry::Diagnostic;
use crate::violations;

const WEAK_HASHES: [&str; 4] = ["md4", "md5", "sha", "sha1"];

fn is_used_for_security(call_args: &SimpleCallArgs) -> bool {
    match call_args.get_argument("usedforsecurity", None) {
        Some(expr) => !matches!(
            &expr.node,
            ExprKind::Constant {
                value: Constant::Bool(false),
                ..
            }
        ),
        _ => true,
    }
}

/// S324
pub fn hashlib_insecure_hash_functions(
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
    from_imports: &FxHashMap<&str, FxHashSet<&str>>,
    import_aliases: &FxHashMap<&str, &str>,
) -> Option<Diagnostic> {
    if match_module_member(func, "hashlib", "new", from_imports, import_aliases) {
        let call_args = SimpleCallArgs::new(args, keywords);

        if !is_used_for_security(&call_args) {
            return None;
        }

        if let Some(name_arg) = call_args.get_argument("name", Some(0)) {
            let hash_func_name = string_literal(name_arg)?;

            if WEAK_HASHES.contains(&hash_func_name.to_lowercase().as_str()) {
                return Some(Diagnostic::new(
                    violations::HashlibInsecureHashFunction(hash_func_name.to_string()),
                    Range::from_located(name_arg),
                ));
            }
        }
    } else {
        for func_name in &WEAK_HASHES {
            if match_module_member(func, "hashlib", func_name, from_imports, import_aliases) {
                let call_args = SimpleCallArgs::new(args, keywords);

                if !is_used_for_security(&call_args) {
                    return None;
                }

                return Some(Diagnostic::new(
                    violations::HashlibInsecureHashFunction((*func_name).to_string()),
                    Range::from_located(func),
                ));
            }
        }
    }
    None
}
