use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr, ExprAttribute, ExprCall};
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::{checkers::ast::Checker, settings::types::PythonVersion};

/// ## What it does
/// Checks for the use of `bin(x).count("1")` as a population count.
///
/// ## Why is this bad?
/// Python 3.10 added the `int.bit_count()` method, which is more efficient.
///
/// ## Example
/// ```python
/// x = bin(123).count("1")
/// y = bin(0b1111011).count("1")
/// ```
///
/// Use instead:
/// ```python
/// x = (123).bit_count()
/// y = 0b1111011.bit_count()
/// ```
#[violation]
pub struct BitCount {
    original_argument: String,
    replacement: String,
}

impl AlwaysFixableViolation for BitCount {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BitCount {
            original_argument, ..
        } = self;
        format!("Use of bin({original_argument}).count('1')")
    }

    fn fix_title(&self) -> String {
        let BitCount { replacement, .. } = self;
        format!("Replace with `{replacement}`")
    }
}

/// FURB161
pub(crate) fn bit_count(checker: &mut Checker, call: &ExprCall) {
    if checker.settings.target_version < PythonVersion::Py310 {
        // `int.bit_count()` was added in Python 3.10
        return;
    }

    let Expr::Attribute(ExprAttribute { attr, value, .. }) = call.func.as_ref() else {
        return;
    };

    // make sure we're doing count
    if attr.as_str() != "count" {
        return;
    }

    let Some(arg) = call.arguments.args.first() else {
        return;
    };

    let Expr::StringLiteral(ast::ExprStringLiteral {
        value: count_value, ..
    }) = arg
    else {
        return;
    };

    // make sure we're doing count("1")
    if count_value != "1" {
        return;
    }

    let Expr::Call(ExprCall {
        func, arguments, ..
    }) = value.as_ref()
    else {
        return;
    };

    let Expr::Name(ast::ExprName { id, .. }) = func.as_ref() else {
        return;
    };

    // make sure we're doing bin()
    if id.as_str() != "bin" {
        return;
    }

    if arguments.len() != 1 {
        return;
    }

    let Some(arg) = arguments.args.first() else {
        return;
    };

    let literal_text = checker.locator().slice(arg.range());

    let replacement = match arg {
        Expr::Name(ast::ExprName { id, .. }) => {
            format!("{id}.bit_count()")
        }
        Expr::NumberLiteral(ast::ExprNumberLiteral { .. }) => {
            let first_two_chars = checker
                .locator()
                .slice(TextRange::new(arg.start(), arg.start() + TextSize::from(2)));

            match first_two_chars {
                "0b" | "0B" | "0x" | "0X" | "0o" | "0O" => format!("{literal_text}.bit_count()"),
                _ => format!("({literal_text}).bit_count()"),
            }
        }
        Expr::Call(ast::ExprCall { .. }) => {
            format!("{literal_text}.bit_count()")
        }
        _ => {
            format!("({literal_text}).bit_count()")
        }
    };

    let mut diagnostic = Diagnostic::new(
        BitCount {
            original_argument: literal_text.to_string(),
            replacement: replacement.clone(),
        },
        call.range(),
    );

    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
        replacement,
        call.range(),
    )));

    checker.diagnostics.push(diagnostic);
}
