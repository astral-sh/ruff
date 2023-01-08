use rustpython_ast::{Expr, Keyword};

use super::helpers::{is_empty_or_null_string, is_pytest_fail};
use crate::ast::helpers::SimpleCallArgs;
use crate::ast::types::Range;
use crate::registry::Diagnostic;
use crate::violations;
use crate::xxxxxxxxs::ast::xxxxxxxx;

pub fn fail_call(xxxxxxxx: &mut xxxxxxxx, call: &Expr, args: &[Expr], keywords: &[Keyword]) {
    if is_pytest_fail(call, xxxxxxxx) {
        let call_args = SimpleCallArgs::new(args, keywords);
        let msg = call_args.get_argument("msg", Some(0));

        if let Some(msg) = msg {
            if is_empty_or_null_string(msg) {
                xxxxxxxx.diagnostics.push(Diagnostic::new(
                    violations::FailWithoutMessage,
                    Range::from_located(call),
                ));
            }
        } else {
            xxxxxxxx.diagnostics.push(Diagnostic::new(
                violations::FailWithoutMessage,
                Range::from_located(call),
            ));
        }
    }
}
