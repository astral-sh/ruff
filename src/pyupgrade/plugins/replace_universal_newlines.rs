use rustpython_ast::{Expr, Keyword, Located};

use crate::ast::helpers::match_module_member;
use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::checks::{Check, CheckKind};

/// UP021
pub fn replace_universal_newlines(checker: &mut Checker, expr: &Expr, kwargs: &[Keyword]) {
    if match_module_member(
        expr,
        "subprocess",
        "run",
        &checker.from_imports,
        &checker.import_aliases,
    ) {
        let filtered_kwargs = kwargs
            .iter()
            .filter(|kw| kw.node.arg == Some("universal_newlines".to_string()))
            .collect::<Vec<_>>();
        if filtered_kwargs.is_empty() {
            return;
        }
        // The kwarg end location includes the value, which we do not want to
        // remove, so we need to find the start of the next value, and then
        // move one to the left so that the '=' sign is not removed.
        let the_kwarg = filtered_kwargs.get(0).unwrap();
        let start = the_kwarg.location;
        let mut end = the_kwarg.node.value.location;
        end.go_left();
        let location: Located<u8> = Located::new(start, end, 0);
        let mut check = Check::new(
            CheckKind::ReplaceUniversalNewlines,
            Range::from_located(&location),
        );
        if checker.patch(check.kind.code()) {
            check.amend(Fix::replacement("text".to_string(), start, end));
        }
        checker.add_check(check);
    }
}
