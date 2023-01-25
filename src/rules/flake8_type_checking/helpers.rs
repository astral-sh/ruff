use rustpython_ast::Expr;

use crate::checkers::ast::Checker;

pub fn is_type_checking_block(checker: &Checker, test: &Expr) -> bool {
    checker.resolve_call_path(test).map_or(false, |call_path| {
        call_path.as_slice() == ["typing", "TYPE_CHECKING"]
    })
}
