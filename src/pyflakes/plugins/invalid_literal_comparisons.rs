use itertools::izip;
use once_cell::unsync::Lazy;
use rustpython_ast::{Cmpop, Expr};

use crate::ast::helpers;
use crate::ast::operations::locate_cmpops;
use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::checks::{Check, CheckKind};

/// F632
pub fn invalid_literal_comparison(
    checker: &mut Checker,
    left: &Expr,
    ops: &[Cmpop],
    comparators: &[Expr],
    location: Range,
) {
    let located = Lazy::new(|| locate_cmpops(&checker.locator.slice_source_code_range(&location)));
    let mut left = left;
    for (index, (op, right)) in izip!(ops, comparators).enumerate() {
        if matches!(op, Cmpop::Is | Cmpop::IsNot)
            && (helpers::is_constant_non_singleton(left)
                || helpers::is_constant_non_singleton(right))
        {
            let mut check = Check::new(CheckKind::IsLiteral, location);
            if checker.patch(check.kind.code()) {
                if let Some(located_op) = &located.get(index) {
                    assert_eq!(&located_op.node, op);
                    if let Some(content) = match &located_op.node {
                        Cmpop::Is => Some("==".to_string()),
                        Cmpop::IsNot => Some("!=".to_string()),
                        node => {
                            eprintln!("Failed to fix invalid comparison: {node:?}");
                            None
                        }
                    } {
                        check.amend(Fix::replacement(
                            content,
                            helpers::to_absolute(located_op.location, location.location),
                            helpers::to_absolute(
                                located_op.end_location.unwrap(),
                                location.location,
                            ),
                        ));
                    }
                } else {
                    eprintln!("Failed to fix invalid comparison due to missing op");
                }
            }
            checker.add_check(check);
        }
        left = right;
    }
}
