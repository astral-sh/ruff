use rustpython_parser::ast::{self, Expr, Ranged, Stmt, WithItem};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum AssertionKind {
    AssertRaises,
    PytestRaises,
}

/// ## What it does
/// Checks for `self.assertRaises(Exception)` or `pytest.raises(Exception)`.
///
/// ## Why is this bad?
/// These forms catch every `Exception`, which can lead to tests passing even
/// if, e.g., the code being tested is never executed due to a typo.
///
/// Either assert for a more specific exception (builtin or custom), or use
/// `assertRaisesRegex` or `pytest.raises(..., match=<REGEX>)` respectively.
///
/// ## Example
/// ```python
/// self.assertRaises(Exception, foo)
/// ```
///
/// Use instead:
/// ```python
/// self.assertRaises(SomeSpecificException, foo)
/// ```
#[violation]
pub struct AssertRaisesException {
    kind: AssertionKind,
}

impl Violation for AssertRaisesException {
    #[derive_message_formats]
    fn message(&self) -> String {
        match self.kind {
            AssertionKind::AssertRaises => {
                format!("`assertRaises(Exception)` should be considered evil")
            }
            AssertionKind::PytestRaises => {
                format!("`pytest.raises(Exception)` should be considered evil")
            }
        }
    }
}

/// B017
pub(crate) fn assert_raises_exception(checker: &mut Checker, stmt: &Stmt, items: &[WithItem]) {
    let Some(item) = items.first() else {
        return;
    };
    let item_context = &item.context_expr;
    let Expr::Call(ast::ExprCall { func, args, keywords, range: _ }) = &item_context else {
        return;
    };
    if args.len() != 1 {
        return;
    }
    if item.optional_vars.is_some() {
        return;
    }

    if !checker
        .semantic()
        .resolve_call_path(args.first().unwrap())
        .map_or(false, |call_path| {
            matches!(call_path.as_slice(), ["", "Exception"])
        })
    {
        return;
    }

    let kind = {
        if matches!(func.as_ref(), Expr::Attribute(ast::ExprAttribute { attr, .. }) if attr == "assertRaises")
        {
            AssertionKind::AssertRaises
        } else if checker
            .semantic()
            .resolve_call_path(func)
            .map_or(false, |call_path| {
                matches!(call_path.as_slice(), ["pytest", "raises"])
            })
            && !keywords
                .iter()
                .any(|keyword| keyword.arg.as_ref().map_or(false, |arg| arg == "match"))
        {
            AssertionKind::PytestRaises
        } else {
            return;
        }
    };

    checker.diagnostics.push(Diagnostic::new(
        AssertRaisesException { kind },
        stmt.range(),
    ));
}
