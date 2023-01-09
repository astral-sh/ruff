use rustc_hash::{FxHashMap, FxHashSet};
use rustpython_ast::{Expr, ExprKind, Keyword};

use crate::ast::helpers::{match_module_member, SimpleCallArgs};
use crate::ast::types::Range;
use crate::registry::Diagnostic;
use crate::violations;

/// S506
pub fn unsafe_yaml_load(
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
    from_imports: &FxHashMap<&str, FxHashSet<&str>>,
    import_aliases: &FxHashMap<&str, &str>,
) -> Option<Diagnostic> {
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
                let loader = match &loader_arg.node {
                    ExprKind::Attribute { attr, .. } => Some(attr.to_string()),
                    ExprKind::Name { id, .. } => Some(id.to_string()),
                    _ => None,
                };
                return Some(Diagnostic::new(
                    violations::UnsafeYAMLLoad(loader),
                    Range::from_located(loader_arg),
                ));
            }
        } else {
            return Some(Diagnostic::new(
                violations::UnsafeYAMLLoad(None),
                Range::from_located(func),
            ));
        }
    }
    None
}
