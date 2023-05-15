use std::fmt;

use rustpython_parser::ast::{self, Constant, Expr, ExprKind, Keyword};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::str::is_implicit_concatenation;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub(crate) enum LiteralType {
    Str,
    Bytes,
}

impl fmt::Display for LiteralType {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            LiteralType::Str => fmt.write_str("str"),
            LiteralType::Bytes => fmt.write_str("bytes"),
        }
    }
}

#[violation]
pub struct NativeLiterals {
    literal_type: LiteralType,
}

impl AlwaysAutofixableViolation for NativeLiterals {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NativeLiterals { literal_type } = self;
        format!("Unnecessary call to `{literal_type}`")
    }

    fn autofix_title(&self) -> String {
        let NativeLiterals { literal_type } = self;
        format!("Replace with `{literal_type}`")
    }
}

/// UP018
pub(crate) fn native_literals(
    checker: &mut Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) {
    let ExprKind::Name(ast::ExprName { id, .. }) = &func.node else { return; };

    if !keywords.is_empty() || args.len() > 1 {
        return;
    }

    if (id == "str" || id == "bytes") && checker.ctx.is_builtin(id) {
        let Some(arg) = args.get(0) else {
            let mut diagnostic = Diagnostic::new(NativeLiterals{literal_type:if id == "str" {
                LiteralType::Str
            } else {
                LiteralType::Bytes
            }}, expr.range());
            if checker.patch(diagnostic.kind.rule()) {
                #[allow(deprecated)]
                diagnostic.set_fix(Fix::unspecified(Edit::range_replacement(
                    if id == "bytes" {
                        let mut content = String::with_capacity(3);
                        content.push('b');
                        content.push(checker.stylist.quote().into());
                        content.push(checker.stylist.quote().into());
                        content
                    } else {
                        let mut content = String::with_capacity(2);
                        content.push(checker.stylist.quote().into());
                        content.push(checker.stylist.quote().into());
                        content
                    },
                    expr.range(),
                )));
            }
            checker.diagnostics.push(diagnostic);
            return;
        };

        // Look for `str("")`.
        if id == "str"
            && !matches!(
                &arg.node,
                ExprKind::Constant(ast::ExprConstant {
                    value: Constant::Str(_),
                    ..
                }),
            )
        {
            return;
        }

        // Look for `bytes(b"")`
        if id == "bytes"
            && !matches!(
                &arg.node,
                ExprKind::Constant(ast::ExprConstant {
                    value: Constant::Bytes(_),
                    ..
                }),
            )
        {
            return;
        }

        // Skip implicit string concatenations.
        let arg_code = checker.locator.slice(arg.range());
        if is_implicit_concatenation(arg_code) {
            return;
        }

        let mut diagnostic = Diagnostic::new(
            NativeLiterals {
                literal_type: if id == "str" {
                    LiteralType::Str
                } else {
                    LiteralType::Bytes
                },
            },
            expr.range(),
        );
        if checker.patch(diagnostic.kind.rule()) {
            #[allow(deprecated)]
            diagnostic.set_fix(Fix::unspecified(Edit::range_replacement(
                arg_code.to_string(),
                expr.range(),
            )));
        }
        checker.diagnostics.push(diagnostic);
    }
}
