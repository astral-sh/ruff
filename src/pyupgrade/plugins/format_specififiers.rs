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

get_specifier_order(value_str: &str) -> Vec<u8> {
}

pub fn format_specifiers(checker: &Checker, expr: &Expr, func: &Expr) {
    if let ExprKind::Attribute { value, attr, .. } = &func.node {
        if let ExprKind::Constant { value: cons_value, .. } = &value.node {
            if attr == "format" {
                println!("{:?}", value);
                println!("{:?}", cons_value);
                println!("{:?}", attr);
                let cons_val_range = Range::from_located(expr);
                let cons_val_text = checker.locator.slice_source_code_range(&cons_val_range);
                get_specifier_order(&cons_val_text);
                let call_range = Range::from_located(expr);
                let call_text = checker.locator.slice_source_code_range(&call_range);
                get_new_call(&call_text);
            }
        }
    }
}
