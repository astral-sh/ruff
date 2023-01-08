use rustpython_ast::{Expr, Keyword, Location};

use crate::ast::helpers::{find_keyword, match_module_member};
use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::registry::Diagnostic;
use crate::violations;
use crate::xxxxxxxxs::ast::xxxxxxxx;

/// UP021
pub fn replace_universal_newlines(xxxxxxxx: &mut xxxxxxxx, expr: &Expr, kwargs: &[Keyword]) {
    if match_module_member(
        expr,
        "subprocess",
        "run",
        &xxxxxxxx.from_imports,
        &xxxxxxxx.import_aliases,
    ) {
        let Some(kwarg) = find_keyword(kwargs, "universal_newlines") else { return; };
        let range = Range::new(
            kwarg.location,
            Location::new(
                kwarg.location.row(),
                kwarg.location.column() + "universal_newlines".len(),
            ),
        );
        let mut check = Diagnostic::new(violations::ReplaceUniversalNewlines, range);
        if xxxxxxxx.patch(check.kind.code()) {
            check.amend(Fix::replacement(
                "text".to_string(),
                range.location,
                range.end_location,
            ));
        }
        xxxxxxxx.diagnostics.push(check);
    }
}
