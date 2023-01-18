use rustpython_ast::{Expr, ExprKind, Keyword};

use crate::ast::helpers::SimpleCallArgs;
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violations;

/// S506
pub fn unsafe_yaml_load(checker: &mut Checker, func: &Expr, args: &[Expr], keywords: &[Keyword]) {
    if checker
        .resolve_call_path(func)
        .map_or(false, |call_path| call_path.as_slice() == ["yaml", "load"])
    {
        let call_args = SimpleCallArgs::new(args, keywords);
        if let Some(loader_arg) = call_args.get_argument("Loader", Some(1)) {
            if !checker
                .resolve_call_path(loader_arg)
                .map_or(false, |call_path| {
                    call_path.as_slice() == ["yaml", "SafeLoader"]
                        || call_path.as_slice() == ["yaml", "CSafeLoader"]
                })
            {
                let loader = match &loader_arg.node {
                    ExprKind::Attribute { attr, .. } => Some(attr.to_string()),
                    ExprKind::Name { id, .. } => Some(id.to_string()),
                    _ => None,
                };
                checker.diagnostics.push(Diagnostic::new(
                    violations::UnsafeYAMLLoad(loader),
                    Range::from_located(loader_arg),
                ));
            }
        } else {
            checker.diagnostics.push(Diagnostic::new(
                violations::UnsafeYAMLLoad(None),
                Range::from_located(func),
            ));
        }
    }
}
