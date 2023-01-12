use rustc_hash::{FxHashMap, FxHashSet};
use rustpython_ast::{Expr, ExprKind, Keyword};
use rustpython_parser::ast::Constant;

use crate::ast::helpers::{collect_call_paths, dealias_call_path, match_call_path, SimpleCallArgs};
use crate::ast::types::Range;
use crate::registry::Diagnostic;
use crate::violations;

/// S701
pub fn jinja2_autoescape_false(
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
    from_imports: &FxHashMap<&str, FxHashSet<&str>>,
    import_aliases: &FxHashMap<&str, &str>,
) -> Option<Diagnostic> {
    let call_path = dealias_call_path(collect_call_paths(func), import_aliases);

    if match_call_path(&call_path, "jinja2", "Environment", from_imports) {
        let call_args = SimpleCallArgs::new(args, keywords);

        if let Some(autoescape_arg) = call_args.get_argument("autoescape", None) {
            match &autoescape_arg.node {
                ExprKind::Constant {
                    value: Constant::Bool(true),
                    ..
                } => (),
                ExprKind::Call { func, .. } => {
                    if let ExprKind::Name { id, .. } = &func.node {
                        if id.as_str() != "select_autoescape" {
                            return Some(Diagnostic::new(
                                violations::Jinja2AutoescapeFalse(true),
                                Range::from_located(autoescape_arg),
                            ));
                        }
                    }
                }
                _ => {
                    return Some(Diagnostic::new(
                        violations::Jinja2AutoescapeFalse(true),
                        Range::from_located(autoescape_arg),
                    ))
                }
            }
        } else {
            return Some(Diagnostic::new(
                violations::Jinja2AutoescapeFalse(false),
                Range::from_located(func),
            ));
        }
    }
    None
}
