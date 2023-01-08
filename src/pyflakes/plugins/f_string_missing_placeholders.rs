use rustpython_ast::{Expr, ExprKind};

use crate::ast::helpers::find_useless_f_strings;
use crate::autofix::Fix;
use crate::registry::{Diagnostic, RuleCode};
use crate::violations;
use crate::xxxxxxxxs::ast::xxxxxxxx;

/// F541
pub fn f_string_missing_placeholders(expr: &Expr, values: &[Expr], xxxxxxxx: &mut xxxxxxxx) {
    if !values
        .iter()
        .any(|value| matches!(value.node, ExprKind::FormattedValue { .. }))
    {
        for (prefix_range, tok_range) in find_useless_f_strings(expr, xxxxxxxx.locator) {
            let mut check = Diagnostic::new(violations::FStringMissingPlaceholders, tok_range);
            if xxxxxxxx.patch(&RuleCode::F541) {
                check.amend(Fix::deletion(
                    prefix_range.location,
                    prefix_range.end_location,
                ));
            }
            xxxxxxxx.diagnostics.push(check);
        }
    }
}
