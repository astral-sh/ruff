use rustc_hash::{FxHashMap, FxHashSet};
use rustpython_ast::{Expr, ExprKind, Keyword};
use rustpython_parser::ast::Constant;

use crate::ast::helpers::{match_module_member, SimpleCallArgs};
use crate::ast::types::Range;
use crate::registry::{Check, CheckKind};

const HTTP_VERBS: [&str; 7] = ["get", "options", "head", "post", "put", "patch", "delete"];

/// S113
pub fn request_without_timeout(
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
    from_imports: &FxHashMap<&str, FxHashSet<&str>>,
    import_aliases: &FxHashMap<&str, &str>,
) -> Option<Check> {
    for func_name in &HTTP_VERBS {
        if match_module_member(func, "requests", func_name, from_imports, import_aliases) {
            let call_args = SimpleCallArgs::new(args, keywords);

            if let Some(timeout_arg) = call_args.get_argument("timeout", None) {
                if let Some(timeout) = match &timeout_arg.node {
                    ExprKind::Constant {
                        value: value @ Constant::None,
                        ..
                    } => Some(value.to_string()),
                    _ => None,
                } {
                    return Some(Check::new(
                        CheckKind::RequestWithoutTimeout(Some(timeout)),
                        Range::from_located(timeout_arg),
                    ));
                }
            } else {
                return Some(Check::new(
                    CheckKind::RequestWithoutTimeout(None),
                    Range::from_located(func),
                ));
            }
        }
    }
    None
}
