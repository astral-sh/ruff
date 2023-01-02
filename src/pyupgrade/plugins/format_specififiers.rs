use libcst_native::{Codegen, CodegenState};
use rustpython_ast::{Expr, ExprKind};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::cst::matchers::{match_call, match_expression};

fn get_new_call(module_text: &str) -> Option<String> {
    let mut expression = match match_expression(&module_text) {
        Err(_) => return None,
        Ok(item) => item,
    };
    let mut call = match match_call(&mut expression) {
        Err(_) => return None,
        Ok(item) => item,
    };
    println!("{:?}", call.args);
    let mut state = CodegenState::default();
    expression.codegen(&mut state);
    Some(state.to_string())
}

pub fn format_specifiers(checker: &Checker, expr: &Expr, func: &Expr) {
    if let ExprKind::Attribute { value, attr, ctx } = &func.node {
        if let ExprKind::Constant { value, kind } = &value.node {
            println!("{:?}", value);
            println!("{:?}", attr);
        }
    }
    let range = Range::from_located(expr);
    let module_text = checker.locator.slice_source_code_range(&range);
    get_new_call(&module_text);
}
