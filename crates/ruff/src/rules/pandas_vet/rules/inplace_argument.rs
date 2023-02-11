use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Constant, Expr, ExprKind, Keyword};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::rules::pandas_vet::fixes::fix_inplace_argument;
use crate::violation::AlwaysAutofixableViolation;

define_violation!(
    pub struct UseOfInplaceArgument;
);
impl AlwaysAutofixableViolation for UseOfInplaceArgument {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`inplace=True` should be avoided; it has inconsistent behavior")
    }

    fn autofix_title(&self) -> String {
        format!("Assign to variable and remove the `inplace` arg")
    }
}

/// PD002
pub fn inplace_argument(
    checker: &Checker,
    expr: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) -> Option<Diagnostic> {
    for keyword in keywords {
        let arg = keyword.node.arg.as_ref()?;

        if arg == "inplace" {
            let is_true_literal = match &keyword.node.value.node {
                ExprKind::Constant {
                    value: Constant::Bool(boolean),
                    ..
                } => *boolean,
                _ => false,
            };
            if is_true_literal {
                let mut diagnostic =
                    Diagnostic::new(UseOfInplaceArgument, Range::from_located(keyword));
                if checker.patch(diagnostic.kind.rule()) {
                    if let Some(fix) = fix_inplace_argument(
                        checker.locator,
                        expr,
                        diagnostic.location,
                        diagnostic.end_location,
                        args,
                        keywords,
                    ) {
                        diagnostic.amend(fix);
                    }
                }
                return Some(diagnostic);
            }
        }
    }
    None
}
