use crate::checkers::ast::Checker;
use rustpython_ast::Expr;

/// UP031
pub fn printf_string_formatting(checker: &mut Checker, left: &Expr, right: &Expr) {
    println!("{:?}", left);
    println!("==========");
    println!("{:?}", right);
}
