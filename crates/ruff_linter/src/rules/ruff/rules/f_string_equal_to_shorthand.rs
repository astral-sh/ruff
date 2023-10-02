use anyhow::{bail, Result};
use ast::{Constant, ExprConstant};
use libcst_native::{
    ConcatenatedString, Expression, FormattedStringContent, FormattedStringExpression,
};

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Arguments, Expr};
use ruff_python_codegen::Stylist;
use ruff_source_file::Locator;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::cst::matchers::{match_call_mut, match_name, transform_expression};
use crate::registry::AsRule;

#[violation]
pub struct FStringEqualToShorthand;

impl AlwaysFixableViolation for FStringEqualToShorthand {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use equal to shorthand")
    }
    fn fix_title(&self) -> String {
        format!("Replace with equal to shorthand")
    }
}

/// RUF018
pub(crate) fn f_string_equal_to_shorthand(checker: &mut Checker, _expr: &Expr, values: &[Expr]) {
    let mut last = None;
    values.into_iter().for_each(|value| match value {
        Expr::Constant(ExprConstant {
            value: Constant::Str(s),
            ..
        }) => last = Some(s.value.as_str()),
        Expr::FormattedValue(expr) => {
            let Some(last_some) = last else { return };
            let src = checker.locator().slice(expr);
            let mut chars = last_some.chars();
            while let Some(c) = chars.next_back() {
                match c {
                    ' ' => continue,
                    '=' => break,
                    _ => {
                        last = None;
                        return;
                    }
                }
            }
            let mut src = src.chars();
            src.next();
            src.next_back();
            if !chars.as_str().trim_end().ends_with(src.as_str()) {
                last = None;
                return;
            }
            checker
                .diagnostics
                .push(Diagnostic::new(FStringEqualToShorthand, expr.range()));
            last = None
        }
        _ => {}
    });
}
