use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr, ExprAttribute, ExprCall};
use ruff_text_size::Ranged;

use crate::fix::snippet::SourceCodeSnippet;
use crate::{checkers::ast::Checker, settings::types::PythonVersion};

/// ## What it does
/// Checks for uses of `bin(...).count("1")` to perform a population count.
///
/// ## Why is this bad?
/// In Python 3.10, a `bit_count()` method was added to the `int` class,
/// which is more concise and efficient than converting to a binary
/// representation via `bin(...)`.
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
///
/// ## Options
/// - `target-version`
///
/// ## References
/// - [Python documentation:`int.bit_count`](https://docs.python.org/3/library/stdtypes.html#int.bit_count)
#[violation]
pub struct BitCount {
    existing: SourceCodeSnippet,
    replacement: SourceCodeSnippet,
}

impl AlwaysFixableViolation for BitCount {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BitCount { existing, .. } = self;
        let existing = existing.truncated_display();
        format!("Use of `bin({existing}).count('1')`")
    }

    fn fix_title(&self) -> String {
        let BitCount { replacement, .. } = self;
        if let Some(replacement) = replacement.full_display() {
            format!("Replace with `{replacement}`")
        } else {
            format!("Replace with `.bit_count()`")
        }
    }
}

/// FURB161
pub(crate) fn bit_count(checker: &mut Checker, call: &ExprCall) {
    // `int.bit_count()` was added in Python 3.10
    if checker.settings.target_version < PythonVersion::Py310 {
        return;
    }

    let Expr::Attribute(ExprAttribute { attr, value, .. }) = call.func.as_ref() else {
        return;
    };

    // Ensure that we're performing a `.count(...)`.
    if attr.as_str() != "count" {
        return;
    }

    if !call.arguments.keywords.is_empty() {
        return;
    };
    let [arg] = &*call.arguments.args else {
        return;
    };

    let Expr::StringLiteral(ast::ExprStringLiteral {
        value: count_value, ..
    }) = arg
    else {
        return;
    };

    // Ensure that we're performing a `.count("1")`.
    if count_value != "1" {
        return;
    }

    let Expr::Call(ExprCall {
        func, arguments, ..
    }) = value.as_ref()
    else {
        return;
    };

    if !arguments.keywords.is_empty() {
        return;
    };
    let [arg] = &*arguments.args else {
        return;
    };

    // Ensure that we're performing a `bin(...)`.
    if !checker.semantic().match_builtin_expr(func, "bin") {
        return;
    }

    // Extract, e.g., `x` in `bin(x)`.
    let literal_text = checker.locator().slice(arg);

    // If we're calling a method on an integer, or an expression with lower precedence, parenthesize
    // it.
    let parenthesize = match arg {
        Expr::NumberLiteral(ast::ExprNumberLiteral { .. }) => {
            let mut chars = literal_text.chars();
            !matches!(
                (chars.next(), chars.next()),
                (Some('0'), Some('b' | 'B' | 'x' | 'X' | 'o' | 'O'))
            )
        }

        Expr::StringLiteral(inner) => inner.value.is_implicit_concatenated(),
        Expr::BytesLiteral(inner) => inner.value.is_implicit_concatenated(),
        Expr::FString(inner) => inner.value.is_implicit_concatenated(),

        Expr::Await(_)
        | Expr::Starred(_)
        | Expr::UnaryOp(_)
        | Expr::BinOp(_)
        | Expr::BoolOp(_)
        | Expr::If(_)
        | Expr::Named(_)
        | Expr::Lambda(_)
        | Expr::Slice(_)
        | Expr::Yield(_)
        | Expr::YieldFrom(_)
        | Expr::Name(_)
        | Expr::List(_)
        | Expr::Compare(_)
        | Expr::Tuple(_)
        | Expr::Generator(_)
        | Expr::IpyEscapeCommand(_) => true,

        Expr::Call(_)
        | Expr::Dict(_)
        | Expr::Set(_)
        | Expr::ListComp(_)
        | Expr::SetComp(_)
        | Expr::DictComp(_)
        | Expr::BooleanLiteral(_)
        | Expr::NoneLiteral(_)
        | Expr::EllipsisLiteral(_)
        | Expr::Attribute(_)
        | Expr::Subscript(_) => false,
    };

    let replacement = if parenthesize {
        format!("({literal_text}).bit_count()")
    } else {
        format!("{literal_text}.bit_count()")
    };

    let mut diagnostic = Diagnostic::new(
        BitCount {
            existing: SourceCodeSnippet::from_str(literal_text),
            replacement: SourceCodeSnippet::new(replacement.to_string()),
        },
        call.range(),
    );

    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
        replacement,
        call.range(),
    )));

    checker.diagnostics.push(diagnostic);
}
