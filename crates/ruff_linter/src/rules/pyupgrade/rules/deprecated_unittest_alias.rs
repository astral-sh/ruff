use once_cell::sync::Lazy;
use ruff_python_ast::{self as ast, Expr};
use rustc_hash::FxHashMap;

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for uses of deprecated methods from the `unittest` module.
///
/// ## Why is this bad?
/// The `unittest` module has deprecated aliases for some of its methods.
/// The deprecated aliases were removed in Python 3.12. Instead of aliases,
/// use their non-deprecated counterparts.
///
/// ## Example
/// ```python
/// from unittest import TestCase
///
///
/// class SomeTest(TestCase):
///     def test_something(self):
///         self.assertEquals(1, 1)
/// ```
///
/// Use instead:
/// ```python
/// from unittest import TestCase
///
///
/// class SomeTest(TestCase):
///     def test_something(self):
///         self.assertEqual(1, 1)
/// ```
///
/// ## References
/// - [Python 3.11 documentation: Deprecated aliases](https://docs.python.org/3.11/library/unittest.html#deprecated-aliases)
#[violation]
pub struct DeprecatedUnittestAlias {
    alias: String,
    target: String,
}

impl AlwaysFixableViolation for DeprecatedUnittestAlias {
    #[derive_message_formats]
    fn message(&self) -> String {
        let DeprecatedUnittestAlias { alias, target } = self;
        format!("`{alias}` is deprecated, use `{target}`")
    }

    fn fix_title(&self) -> String {
        let DeprecatedUnittestAlias { alias, target } = self;
        format!("Replace `{target}` with `{alias}`")
    }
}

static DEPRECATED_ALIASES: Lazy<FxHashMap<&'static str, &'static str>> = Lazy::new(|| {
    FxHashMap::from_iter([
        ("assertAlmostEquals", "assertAlmostEqual"),
        ("assertEquals", "assertEqual"),
        ("assertNotAlmostEquals", "assertNotAlmostEqual"),
        ("assertNotEquals", "assertNotEqual"),
        ("assertNotRegexpMatches", "assertNotRegex"),
        ("assertRaisesRegexp", "assertRaisesRegex"),
        ("assertRegexpMatches", "assertRegex"),
        ("assert_", "assertTrue"),
        ("failIf", "assertFalse"),
        ("failIfAlmostEqual", "assertNotAlmostEqual"),
        ("failIfEqual", "assertNotEqual"),
        ("failUnless", "assertTrue"),
        ("failUnlessAlmostEqual", "assertAlmostEqual"),
        ("failUnlessEqual", "assertEqual"),
        ("failUnlessRaises", "assertRaises"),
    ])
});

/// UP005
pub(crate) fn deprecated_unittest_alias(checker: &mut Checker, expr: &Expr) {
    let Expr::Attribute(ast::ExprAttribute { value, attr, .. }) = expr else {
        return;
    };
    let Some(target) = DEPRECATED_ALIASES.get(attr.as_str()) else {
        return;
    };
    let Expr::Name(ast::ExprName { id, .. }) = value.as_ref() else {
        return;
    };
    if id != "self" {
        return;
    }
    let mut diagnostic = Diagnostic::new(
        DeprecatedUnittestAlias {
            alias: attr.to_string(),
            target: (*target).to_string(),
        },
        expr.range(),
    );
    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
        format!("self.{target}"),
        expr.range(),
    )));
    checker.diagnostics.push(diagnostic);
}
