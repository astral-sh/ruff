use rustpython_parser::ast::{self, Expr, Keyword, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::is_const_none;
use ruff_python_semantic::SemanticModel;

use crate::checkers::ast::Checker;

#[violation]
pub struct ZipWithoutExplicitStrict;

impl Violation for ZipWithoutExplicitStrict {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`zip()` without an explicit `strict=` parameter")
    }
}

/// B905
pub(crate) fn zip_without_explicit_strict(
    checker: &mut Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
    kwargs: &[Keyword],
) {
    if let Expr::Name(ast::ExprName { id, .. }) = func {
        if id == "zip"
            && checker.semantic_model().is_builtin("zip")
            && !kwargs
                .iter()
                .any(|keyword| keyword.arg.as_ref().map_or(false, |name| name == "strict"))
            && !args
                .iter()
                .any(|arg| is_infinite_iterator(arg, checker.semantic_model()))
        {
            checker
                .diagnostics
                .push(Diagnostic::new(ZipWithoutExplicitStrict, expr.range()));
        }
    }
}

/// Return `true` if the [`Expr`] appears to be an infinite iterator (e.g., a call to
/// `itertools.cycle` or similar).
fn is_infinite_iterator(arg: &Expr, model: &SemanticModel) -> bool {
    let Expr::Call(ast::ExprCall { func, args, keywords, .. }) = &arg else {
        return false;
    };

    return model
        .resolve_call_path(func)
        .map_or(false, |call_path| match call_path.as_slice() {
            ["itertools", "cycle" | "count"] => true,
            ["itertools", "repeat"] => {
                // Ex) `itertools.repeat(1)`
                if keywords.is_empty() && args.len() == 1 {
                    return true;
                }

                // Ex) `itertools.repeat(1, None)`
                if args.len() == 2 && is_const_none(&args[1]) {
                    return true;
                }

                // Ex) `iterools.repeat(1, times=None)`
                for keyword in keywords {
                    if keyword.arg.as_ref().map_or(false, |name| name == "times") {
                        if is_const_none(&keyword.value) {
                            return true;
                        }
                    }
                }

                false
            }
            _ => false,
        });
}
