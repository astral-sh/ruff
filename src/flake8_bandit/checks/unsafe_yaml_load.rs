use rustc_hash::{FxHashMap, FxHashSet};
use rustpython_ast::{Expr, ExprKind, Keyword};

use crate::ast::helpers::{match_module_member, SimpleCallArgs};
use crate::ast::types::Range;
use crate::registry::{Check, CheckKind};

/// S506
pub fn unsafe_yaml_load(
    func: &Expr,
    args: &Vec<Expr>,
    keywords: &Vec<Keyword>,
    from_imports: &FxHashMap<&str, FxHashSet<&str>>,
    import_aliases: &FxHashMap<&str, &str>,
) -> Option<Check> {
    if match_module_member(func, "yaml", "load", from_imports, import_aliases) {
        let call_args = SimpleCallArgs::new(args, keywords);

        if let Some(loader_arg) = call_args.get_argument("Loader", Some(1)) {
            if !match_module_member(
                loader_arg,
                "yaml",
                "SafeLoader",
                from_imports,
                import_aliases,
            ) && !match_module_member(
                loader_arg,
                "yaml",
                "CSafeLoader",
                from_imports,
                import_aliases,
            ) {
                let loader_name = match &loader_arg.node {
                    ExprKind::Attribute { attr, .. } => Some(attr),
                    ExprKind::Name { id, .. } => Some(id),
                    _ => None,
                }?;
                return Some(Check::new(
                    CheckKind::UnsafeYAMLLoad(loader_name.clone()),
                    Range::from_located(loader_arg),
                ));
            }
        } else {
            return Some(Check::new(
                CheckKind::UnsafeYAMLLoad("No Loader used".to_string()),
                Range::from_located(func),
            ));
        }
    }
    None
}
