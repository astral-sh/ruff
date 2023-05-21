use once_cell::sync::Lazy;
use rustc_hash::FxHashMap;
use rustpython_parser::ast::{self, Expr};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::traits::Analyzer;
use crate::checkers::ast::RuleContext;
use crate::registry::{AsRule, Rule};

#[violation]
pub struct DeprecatedUnittestAlias {
    alias: String,
    target: String,
}

impl AlwaysAutofixableViolation for DeprecatedUnittestAlias {
    #[derive_message_formats]
    fn message(&self) -> String {
        let DeprecatedUnittestAlias { alias, target } = self;
        format!("`{alias}` is deprecated, use `{target}`")
    }

    fn autofix_title(&self) -> String {
        let DeprecatedUnittestAlias { alias, target } = self;
        format!("Replace `{target}` with `{alias}`")
    }
}

impl Analyzer<ast::ExprCall> for DeprecatedUnittestAlias {
    fn rule() -> Rule {
        Rule::DeprecatedUnittestAlias
    }

    fn run(diagnostics: &mut Vec<Diagnostic>, checker: &RuleContext, node: &ast::ExprCall) {
        deprecated_unittest_alias(diagnostics, checker, node);
    }
}

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
pub(crate) fn deprecated_unittest_alias(
    diagnostics: &mut Vec<Diagnostic>,
    checker: &RuleContext,
    ast::ExprCall { func, .. }: &ast::ExprCall,
) {
    let Expr::Attribute(ast::ExprAttribute { value, attr, range, .. }) = func.as_ref() else {
        return;
    };
    let Expr::Name(ast::ExprName { id, .. }) = value.as_ref() else {
        return;
    };
    if id != "self" {
        return;
    }
    let Some(target) = DEPRECATED_ALIASES.get(attr.as_str()) else {
        return;
    };
    let mut diagnostic = Diagnostic::new(
        DeprecatedUnittestAlias {
            alias: attr.to_string(),
            target: (*target).to_string(),
        },
        *range,
    );
    if checker.patch(diagnostic.kind.rule()) {
        #[allow(deprecated)]
        diagnostic.set_fix(Fix::unspecified(Edit::range_replacement(
            format!("self.{target}"),
            *range,
        )));
    }
    diagnostics.push(diagnostic);
}
