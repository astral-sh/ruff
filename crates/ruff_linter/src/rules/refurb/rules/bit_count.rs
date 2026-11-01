use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{ExprCall, ExprKind, PythonVersion};
use ruff_python_semantic::analyze::type_inference::{NumberLike, PythonType, ResolvedPythonType};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::fix::snippet::SourceCodeSnippet;
use crate::{AlwaysFixableViolation, Applicability, Edit, Fix};

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
/// ## Fix safety
/// This rule's fix is marked as unsafe unless the argument to `bin` can be inferred as
/// an instance of a type that implements the `__index__` and `bit_count` methods because this can
/// change the exception raised at runtime for an invalid argument.
///
/// ## Options
/// - `target-version`
///
/// ## References
/// - [Python documentation:`int.bit_count`](https://docs.python.org/3/library/stdtypes.html#int.bit_count)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "0.5.0", category = Category::Complexity)]
pub(crate) struct BitCount {
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
            "Replace with `.bit_count()`".to_string()
        }
    }
}

/// FURB161
pub(crate) fn bit_count(checker: &Checker, call: ExprCall<'_>) {
    // `int.bit_count()` was added in Python 3.10
    if checker.target_version() < PythonVersion::PY310 {
        return;
    }

    let ExprKind::Attribute(attribute) = call.func().classify() else {
        return;
    };
    let attr = attribute.attr();
    let value = attribute.value();

    // Ensure that we're performing a `.count(...)`.
    if attr.as_str() != "count" {
        return;
    }

    if !call.arguments().keywords().is_empty() {
        return;
    }
    let Some([arg]) = call.arguments().args().as_array() else {
        return;
    };

    let ExprKind::StringLiteral(string_literal) = arg.classify() else {
        return;
    };
    let count_value = string_literal.value();

    // Ensure that we're performing a `.count("1")`.
    if count_value != "1" {
        return;
    }

    let ExprKind::Call(call_node) = value.classify() else {
        return;
    };
    let func = call_node.func();
    let arguments = call_node.arguments();

    if !arguments.keywords().is_empty() {
        return;
    }
    let Some([arg]) = arguments.args().as_array() else {
        return;
    };

    // Ensure that we're performing a `bin(...)`.
    if !checker.semantic().match_builtin_expr(func, "bin") {
        return;
    }

    // If is a starred expression, it returns.
    if arg.is_starred_expr() {
        return;
    }
    // Extract, e.g., `x` in `bin(x)`.
    let literal_text = checker.locator().slice(arg);

    // If we're calling a method on an integer, or an expression with lower precedence, parenthesize
    // it.
    let parenthesize = match arg.classify() {
        ExprKind::NumberLiteral(_) => {
            let mut chars = literal_text.chars();
            !matches!(
                (chars.next(), chars.next()),
                (Some('0'), Some('b' | 'B' | 'x' | 'X' | 'o' | 'O'))
            )
        }

        ExprKind::StringLiteral(inner) => inner.value().is_implicit_concatenated(),
        ExprKind::BytesLiteral(inner) => inner.value().is_implicit_concatenated(),
        ExprKind::FString(inner) => inner.value().is_implicit_concatenated(),
        ExprKind::TString(inner) => inner.value().is_implicit_concatenated(),

        ExprKind::Await(_)
        | ExprKind::Starred(_)
        | ExprKind::UnaryOp(_)
        | ExprKind::BinOp(_)
        | ExprKind::BoolOp(_)
        | ExprKind::If(_)
        | ExprKind::Named(_)
        | ExprKind::Lambda(_)
        | ExprKind::Slice(_)
        | ExprKind::Yield(_)
        | ExprKind::YieldFrom(_)
        | ExprKind::Name(_)
        | ExprKind::List(_)
        | ExprKind::Compare(_)
        | ExprKind::Tuple(_)
        | ExprKind::Generator(_)
        | ExprKind::IpyEscapeCommand(_) => true,

        ExprKind::Call(_)
        | ExprKind::Dict(_)
        | ExprKind::Set(_)
        | ExprKind::ListComp(_)
        | ExprKind::SetComp(_)
        | ExprKind::DictComp(_)
        | ExprKind::BooleanLiteral(_)
        | ExprKind::NoneLiteral(_)
        | ExprKind::EllipsisLiteral(_)
        | ExprKind::Attribute(_)
        | ExprKind::Subscript(_) => false,
    };

    // check if the fix is safe or not
    let applicability: Applicability = match ResolvedPythonType::from(arg) {
        ResolvedPythonType::Atom(PythonType::Number(NumberLike::Integer | NumberLike::Bool)) => {
            Applicability::Safe
        }
        _ => Applicability::Unsafe,
    };

    let replacement = if parenthesize {
        format!("({literal_text}).bit_count()")
    } else {
        format!("{literal_text}.bit_count()")
    };

    let mut diagnostic = checker.report_diagnostic(
        BitCount {
            existing: SourceCodeSnippet::from_str(literal_text),
            replacement: SourceCodeSnippet::new(replacement.clone()),
        },
        call.range(),
    );
    diagnostic.set_fix(Fix::applicable_edit(
        Edit::range_replacement(replacement, call.range()),
        applicability,
    ));
}
