use once_cell::sync::Lazy;
use rustc_hash::FxHashMap;
use rustpython_ast::{Expr, ExprKind};

use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::checks::{Check, CheckKind};

static DEPRECATED_ALIASES: Lazy<FxHashMap<&'static str, &'static str>> = Lazy::new(|| {
    FxHashMap::from_iter([
        ("failUnlessEqual", "assertEqual"),
        ("assertEquals", "assertEqual"),
        ("failIfEqual", "assertNotEqual"),
        ("assertNotEquals", "assertNotEqual"),
        ("failUnless", "assertTrue"),
        ("assert_", "assertTrue"),
        ("failIf", "assertFalse"),
        ("failUnlessRaises", "assertRaises"),
        ("failUnlessAlmostEqual", "assertAlmostEqual"),
        ("assertAlmostEquals", "assertAlmostEqual"),
        ("failIfAlmostEqual", "assertNotAlmostEqual"),
        ("assertNotAlmostEquals", "assertNotAlmostEqual"),
        ("assertRegexpMatches", "assertRegex"),
        ("assertNotRegexpMatches", "assertNotRegex"),
        ("assertRaisesRegexp", "assertRaisesRegex"),
    ])
});

/// UP005
pub fn deprecated_unittest_alias(checker: &mut Checker, expr: &Expr) {
    let ExprKind::Attribute { value, attr, .. } = &expr.node else {
        return;
    };
    let Some(&target) = DEPRECATED_ALIASES.get(attr.as_str()) else {
        return;
    };
    let ExprKind::Name { id, .. } = &value.node else {
        return;
    };
    if id != "self" {
        return;
    }
    let mut check = Check::new(
        CheckKind::DeprecatedUnittestAlias(attr.to_string(), target.to_string()),
        Range::from_located(expr),
    );
    if checker.patch(check.kind.code()) {
        check.amend(Fix::replacement(
            format!("self.{target}"),
            expr.location,
            expr.end_location.unwrap(),
        ));
    }
    checker.add_check(check);
}
