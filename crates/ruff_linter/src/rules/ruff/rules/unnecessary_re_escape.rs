use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{BytesLiteralValue, Expr, ExprBytesLiteral, ExprCall, ExprStringLiteral};
use ruff_python_semantic::{Modules, SemanticModel};
use ruff_text_size::TextRange;

use crate::checkers::ast::Checker;
use crate::rules::ruff::rules::string_has_metacharacters;

/// ## What it does
/// Checks for `re.escape()` calls that does not change the given value.
///
/// ## Why is this bad?
/// `re.escape()` converts a string to an escaped version
/// that will match the original string when compiled as a regex.
/// Strings that contain no special characters already match themselves
/// and thus require no escaping.
///
/// ## Example
///
/// ```python
/// foo = re.escape('bar')
/// ```
///
/// Use instead:
///
/// ```python
/// foo = 'bar'
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct UnnecessaryReEscape;

impl AlwaysFixableViolation for UnnecessaryReEscape {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Pattern does not need escaping".to_string()
    }

    fn fix_title(&self) -> String {
        "Remove `re.escape()` wrapper call".to_string()
    }
}

/// RUF047
pub(crate) fn unnecessary_re_escape(checker: &mut Checker, call: &ExprCall) {
    if !is_re_escape(checker.semantic(), call) {
        return;
    }

    let Some(argument) = pattern_argument(call) else {
        return;
    };

    let (parts_count, literal_range) = match argument {
        Expr::StringLiteral(ExprStringLiteral { value, range })
            if !string_has_metacharacters(value) =>
        {
            (value.iter().len(), range)
        }

        Expr::BytesLiteral(ExprBytesLiteral { value, range })
            if !bytes_has_metacharacters(value) =>
        {
            (value.iter().len(), range)
        }

        _ => return,
    };

    let fix = replace_with_argument_fix(checker, call, parts_count, literal_range);
    let diagnostic = Diagnostic::new(UnnecessaryReEscape, call.range);

    checker.diagnostics.push(diagnostic.with_fix(fix))
}

fn replace_with_argument_fix(
    checker: &mut Checker,
    call: &ExprCall,
    parts_count: usize,
    literal_range: &TextRange,
) -> Fix {
    let literal_expr = checker.locator().slice(literal_range);
    let replacement = parenthesize_if_necessary(literal_expr, parts_count);
    let edit = Edit::range_replacement(replacement, call.range);

    Fix::safe_edit(edit)
}

#[inline]
fn parenthesize_if_necessary(literal_expr: &str, parts_count: usize) -> String {
    if parts_count > 1 {
        format!("({literal_expr})")
    } else {
        format!("{literal_expr}")
    }
}

fn is_re_escape(semantic: &SemanticModel, call: &ExprCall) -> bool {
    if !semantic.seen_module(Modules::RE) {
        return false;
    }

    let Some(qualified_name) = semantic.resolve_qualified_name(&call.func) else {
        return false;
    };

    matches!(qualified_name.segments(), ["re", "escape"])
}

fn pattern_argument(call: &ExprCall) -> Option<&Expr> {
    let arguments = &call.arguments;

    if arguments.len() > 1 || !arguments.keywords.is_empty() {
        return None;
    }

    arguments.find_argument("pattern", 0)
}

/// See also [`string_has_metacharacters`].
pub(crate) fn bytes_has_metacharacters(value: &BytesLiteralValue) -> bool {
    value.bytes().any(is_metacharacter)
}

#[inline]
fn is_metacharacter(codepoint: u8) -> bool {
    matches!(
        codepoint,
        // ['.', '^', '$', '*', '+', '?', '{', '[', '\\', '|', '(']
        0x2E | 0x5E | 0x24 | 0x2A | 0x2B | 0x3F | 0x7B | 0x5B | 0x5C | 0x7C | 0x28
    )
}
