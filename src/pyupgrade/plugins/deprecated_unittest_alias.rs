use std::collections::BTreeMap;

use once_cell::sync::Lazy;
use rustpython_ast::{Expr, ExprKind};

use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::check_ast::Checker;
use crate::checks::{Check, CheckKind};

static DEPRECATED_ALIASES: Lazy<BTreeMap<&'static str, &'static str>> = Lazy::new(|| {
    BTreeMap::from([
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

pub fn deprecated_unittest_alias(checker: &mut Checker, expr: &Expr) {
    if let ExprKind::Attribute { value, attr, .. } = &expr.node {
        if let Some(target) = DEPRECATED_ALIASES.get(attr.as_str()) {
            if let ExprKind::Name { id, .. } = &value.node {
                if id == "self" {
                    let mut check = Check::new(
                        CheckKind::DeprecatedUnittestAlias(attr.to_string(), target.to_string()),
                        Range::from_located(expr),
                    );
                    if checker.patch() {
                        check.amend(Fix::replacement(
                            format!("self.{}", target),
                            expr.location,
                            expr.end_location.unwrap(),
                        ));
                    }
                    checker.add_check(check);
                }
            }
        }
    }
}
