use rustpython_ast::{Expr, Keyword};

use super::helpers::{is_empty_or_null_string, is_pytest_fail};
use crate::ast::helpers::SimpleCallArgs;
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::{Check, CheckKind};

pub fn fail_call(checker: &mut Checker, call: &Expr, args: &Vec<Expr>, keywords: &Vec<Keyword>) {
    if is_pytest_fail(call, checker) {
        let call_args = SimpleCallArgs::new(args, keywords);
        let msg = call_args.get_argument("msg", Some(0));

        if let Some(msg) = msg {
            if is_empty_or_null_string(msg) {
                checker.add_check(Check::new(
                    CheckKind::FailWithoutMessage,
                    Range::from_located(call),
                ));
            }
        } else {
            checker.add_check(Check::new(
                CheckKind::FailWithoutMessage,
                Range::from_located(call),
            ));
        }
    }
}
