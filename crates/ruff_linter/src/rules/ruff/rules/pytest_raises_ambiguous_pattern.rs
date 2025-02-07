use crate::checkers::ast::Checker;
use crate::rules::flake8_pytest_style::rules::is_pytest_raises;
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast as ast;

/// ## What it does
/// Checks for non-raw literal string arguments passed to the `match` parameter
/// of `pytest.raises()` where the string contains at least one unescaped
/// regex metacharacter.
///
/// ## Why is this bad?
/// The `match` argument is implicitly converted to a regex under the hood.
/// It should be made explicit whether the string is meant to be a regex or a "plain" pattern
/// by prefixing the string with the `r` suffix, escaping the metacharacter(s)
/// in the string using backslashes, or wrapping the entire string in a call to
/// `re.escape()`.
///
/// ## Example
///
/// ```python
/// import pytest
///
///
/// with pytest.raises(Exception, match="A full sentence."):
///     do_thing_that_raises()
/// ```
///
/// Use instead:
///
/// ```python
/// import pytest
///
///
/// with pytest.raises(Exception, match=r"A full sentence."):
///     do_thing_that_raises()
/// ```
///
/// Alternatively:
///
/// ```python
/// import pytest
/// import re
///
///
/// with pytest.raises(Exception, match=re.escape("A full sentence.")):
///     do_thing_that_raises()
/// ```
///
/// or:
///
/// ```python
/// import pytest
/// import re
///
///
/// with pytest.raises(Exception, "A full sentence\\."):
///     do_thing_that_raises()
/// ```
///
/// ## References
/// - [Python documentation: `re.escape`](https://docs.python.org/3/library/re.html#re.escape)
/// - [`pytest` documentation: `pytest.raises`](https://docs.pytest.org/en/latest/reference/reference.html#pytest-raises)
#[derive(ViolationMetadata)]
pub(crate) struct PytestRaisesAmbiguousPattern;

impl Violation for PytestRaisesAmbiguousPattern {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Pattern passed to `match=` contains metacharacters but is neither escaped nor raw"
            .to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Use a raw string or `re.escape()` to make the intention explicit".to_string())
    }
}

/// RUF043
pub(crate) fn pytest_raises_ambiguous_pattern(checker: &Checker, call: &ast::ExprCall) {
    if !is_pytest_raises(&call.func, checker.semantic()) {
        return;
    }

    // It *can* be passed as a positional argument if you try very hard,
    // but pytest only documents it as a keyword argument, and it's quite hard pass it positionally
    let Some(ast::Keyword { value, .. }) = call.arguments.find_keyword("match") else {
        return;
    };

    let ast::Expr::StringLiteral(string) = value else {
        return;
    };

    let any_part_is_raw = string.value.iter().any(|part| part.flags.prefix().is_raw());

    if any_part_is_raw || !string_has_unescaped_metacharacters(&string.value) {
        return;
    }

    let diagnostic = Diagnostic::new(PytestRaisesAmbiguousPattern, string.range);

    checker.report_diagnostic(diagnostic);
}

fn string_has_unescaped_metacharacters(value: &ast::StringLiteralValue) -> bool {
    let mut escaped = false;

    for character in value.chars() {
        if escaped {
            if escaped_char_is_regex_metasequence(character) {
                return true;
            }

            escaped = false;
            continue;
        }

        if character == '\\' {
            escaped = true;
            continue;
        }

        if char_is_regex_metacharacter(character) {
            return true;
        }
    }

    false
}

/// Whether the sequence `\<c>` means anything special:
///
/// * `\A`: Start of input
/// * `\b`, `\B`: Word boundary and non-word-boundary
/// * `\d`, `\D`: Digit and non-digit
/// * `\s`, `\S`: Whitespace and non-whitespace
/// * `\w`, `\W`: Word and non-word character
/// * `\z`: End of input
///
/// `\u`, `\U`, `\N`, `\x`, `\a`, `\f`, `\n`, `\r`, `\t`, `\v`
/// are also valid in normal strings and thus do not count.
/// `\b` means backspace only in character sets,
/// while backreferences (e.g., `\1`) are not valid without groups,
/// both of which should be caught in [`string_has_unescaped_metacharacters`].
const fn escaped_char_is_regex_metasequence(c: char) -> bool {
    matches!(c, 'A' | 'b' | 'B' | 'd' | 'D' | 's' | 'S' | 'w' | 'W' | 'z')
}

const fn char_is_regex_metacharacter(c: char) -> bool {
    matches!(
        c,
        '.' | '^' | '$' | '*' | '+' | '?' | '{' | '[' | '\\' | '|' | '(' | ')'
    )
}
