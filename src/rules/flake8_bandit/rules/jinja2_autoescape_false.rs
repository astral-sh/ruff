use rustpython_ast::{Expr, ExprKind, Keyword};
use rustpython_parser::ast::Constant;

use crate::ast::helpers::SimpleCallArgs;
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violations;

/// S701
pub fn jinja2_autoescape_false(
    checker: &mut Checker,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) {
    if checker.resolve_call_path(func).map_or(false, |call_path| {
        call_path.as_slice() == ["jinja2", "Environment"]
    }) {
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
                            checker.diagnostics.push(Diagnostic::new(
                                violations::Jinja2AutoescapeFalse(true),
                                Range::from_located(autoescape_arg),
                            ));
                        }
                    }
                }
                _ => checker.diagnostics.push(Diagnostic::new(
                    violations::Jinja2AutoescapeFalse(true),
                    Range::from_located(autoescape_arg),
                )),
            }
        } else {
            checker.diagnostics.push(Diagnostic::new(
                violations::Jinja2AutoescapeFalse(false),
                Range::from_located(func),
            ));
        }
    }
}
