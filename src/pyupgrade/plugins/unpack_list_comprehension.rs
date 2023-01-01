use rustpython_ast::{Expr, ExprKind};

use crate::checks::{Check, CheckCode, CheckKind};
use crate::checkers::ast::Checker;
use crate::ast::types::Range;
use crate::autofix::Fix;

/// UP027
pub fn unpack_list_comprehension(checker: &mut Checker, targets: &[Expr], value: &Expr) {
    let target = match targets.get(0) {
        Some(target) => target,
        None => return,
    };
    if let ExprKind::Tuple{ .. } = target.node {
        if let ExprKind::ListComp { .. } = &value.node {
            println!("{:?}\n", value);
            let the_range = Range::new(value.location, value.end_location.unwrap());
            let mut the_text = checker.locator.slice_source_code_range(&the_range).to_string();
            let mut new_string = String::new();
            new_string.push('(');
            // Get middle of old string and push it
            the_text.pop();
            new_string.push_str(&the_text[1..]);
            new_string.push(')');
            let mut check = Check::new(CheckKind::RewriteListComprehension, Range::from_located(value));
            if checker.patch(&CheckCode::UP020) {
                check.amend(Fix::replacement(
                    new_string,
                    value.location,
                    value.end_location.unwrap(),
                ));
            }
            checker.add_check(check);
        }
    }
}
