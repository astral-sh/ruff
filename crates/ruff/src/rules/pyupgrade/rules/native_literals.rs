use std::fmt;

use rustpython_parser::ast::{self, Constant, Expr, Keyword, Ranged};

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

/// ## What it does
/// Checks for unnecessary calls to `str` and `bytes`.
///
/// ## Why is this bad?
/// The `str` and `bytes` constructors can be replaced with string and bytes
/// literals, which are more readable and idiomatic.
///
/// ## Example
/// ```python
/// str("foo")
/// ```
///
/// Use instead:
/// ```python
/// "foo"
/// ```
///
/// ## References
/// - [Python documentation: `str`](https://docs.python.org/3/library/stdtypes.html#str)
/// - [Python documentation: `bytes`](https://docs.python.org/3/library/stdtypes.html#bytes)
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
        match literal_type {
            LiteralType::Str => "Replace with empty string".to_string(),
            LiteralType::Bytes => "Replace with empty bytes".to_string(),
        }
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
    let Expr::Name(ast::ExprName { id, .. }) = func else {
        return;
    };

    if !keywords.is_empty() || args.len() > 1 {
        return;
    }

    // There's no way to rewrite, e.g., `f"{f'{str()}'}"` within a nested f-string.
    if checker.semantic().in_nested_f_string() {
        return;
    }

    if (id == "str" || id == "bytes") && checker.semantic().is_builtin(id) {
        let Some(arg) = args.get(0) else {
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
                let constant = if id == "bytes" {
                    Constant::Bytes(vec![])
                } else {
                    Constant::Str(String::new())
                };
                let content = checker.generator().constant(&constant);
                diagnostic.set_fix(Fix::automatic(Edit::range_replacement(
                    content,
                    expr.range(),
                )));
            }
            checker.diagnostics.push(diagnostic);
            return;
        };

        // Look for `str("")`.
        if id == "str"
            && !matches!(
                &arg,
                Expr::Constant(ast::ExprConstant {
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
                &arg,
                Expr::Constant(ast::ExprConstant {
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
            diagnostic.set_fix(Fix::automatic(Edit::range_replacement(
                arg_code.to_string(),
                expr.range(),
            )));
        }
        checker.diagnostics.push(diagnostic);
    }
}
