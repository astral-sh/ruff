use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;
use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{CmpOp, Expr, ExprCompare, ExprStringLiteral};
use ruff_text_size::TextRange;

/// ## What it does
/// Checks for uses of hardcoded charsets, which are defined in Python string module.
///
/// ## Why is this bad?
/// Usage of named charsets from the standard library is more readable and less error-prone.
///
/// ## Example
/// ```python
/// x = "0123456789"
/// y in "abcdefghijklmnopqrstuvwxyz"
/// ```
///
/// Use instead
/// ```python
/// import string
///
/// x = string.digits
/// y in string.ascii_lowercase
/// ```
///
/// ## References
/// - [Python documentation: String constants](https://docs.python.org/3/library/string.html#string-constants)
#[violation]
pub struct HardcodedStringCharset {
    name: &'static str,
}

impl AlwaysFixableViolation for HardcodedStringCharset {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use of hardcoded string charset")
    }

    fn fix_title(&self) -> String {
        let HardcodedStringCharset { name } = self;
        format!("Replace hardcoded charset with `string.{name}`")
    }
}

struct NamedCharset {
    name: &'static str,
    bytes: &'static [u8],
    ascii_char_set: AsciiCharSet,
}

/// Represents the set of ascii characters in form of a bitset.
#[derive(Copy, Clone, Eq, PartialEq)]
struct AsciiCharSet(u128);

impl AsciiCharSet {
    /// Creates the set of ascii characters from `bytes`.
    /// Returns None if there is non-ascii byte.
    const fn from_bytes(bytes: &[u8]) -> Option<Self> {
        // TODO: simplify implementation, when const-traits are supported
        //  https://github.com/rust-lang/rust-project-goals/issues/106
        let mut bitset = 0;
        let mut i = 0;
        while i < bytes.len() {
            if !bytes[i].is_ascii() {
                return None;
            }
            bitset |= 1 << bytes[i];
            i += 1;
        }
        Some(Self(bitset))
    }
}

impl NamedCharset {
    const fn new(name: &'static str, bytes: &'static [u8]) -> Self {
        Self {
            name,
            bytes,
            // SAFETY: The named charset is guaranteed to have only ascii bytes.
            // TODO: replace with `.unwrap()`, when `Option::unwrap` will be stable in `const fn`
            //  https://github.com/rust-lang/rust/issues/67441
            ascii_char_set: match AsciiCharSet::from_bytes(bytes) {
                Some(ascii_char_set) => ascii_char_set,
                None => unreachable!(),
            },
        }
    }
}

const KNOWN_NAMED_CHARSETS: [NamedCharset; 9] = [
    NamedCharset::new(
        "ascii_letters",
        b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ",
    ),
    NamedCharset::new("ascii_lowercase", b"abcdefghijklmnopqrstuvwxyz"),
    NamedCharset::new("ascii_uppercase", b"ABCDEFGHIJKLMNOPQRSTUVWXYZ"),
    NamedCharset::new("digits", b"0123456789"),
    NamedCharset::new("hexdigits", b"0123456789abcdefABCDEF"),
    NamedCharset::new("octdigits", b"01234567"),
    NamedCharset::new("punctuation", b"!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~"),
    NamedCharset::new(
        "printable",
        b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ!\"\
        #$%&'()*+,-./:;<=>?@[\\]^_`{|}~ \t\n\r\x0b\x0c",
    ),
    NamedCharset::new("whitespace", b" \t\n\r\x0b\x0c"),
];

fn check_charset_as_set(bytes: &[u8]) -> Option<&NamedCharset> {
    let ascii_char_set = AsciiCharSet::from_bytes(bytes)?;

    KNOWN_NAMED_CHARSETS
        .iter()
        .find(|&charset| charset.ascii_char_set == ascii_char_set)
}

fn check_charset_exact(bytes: &[u8]) -> Option<&NamedCharset> {
    KNOWN_NAMED_CHARSETS
        .iter()
        .find(|&charset| charset.bytes == bytes)
}

fn push_diagnostic(checker: &mut Checker, range: TextRange, charset: &NamedCharset) {
    let name = charset.name;
    let mut diagnostic = Diagnostic::new(HardcodedStringCharset { name }, range);
    diagnostic.try_set_fix(|| {
        let (edit, binding) = checker.importer().get_or_import_symbol(
            &ImportRequest::import("string", name),
            range.start(),
            checker.semantic(),
        )?;
        Ok(Fix::safe_edits(
            Edit::range_replacement(binding, range),
            [edit],
        ))
    });
    checker.diagnostics.push(diagnostic);
}

// Helper function to check if the replacement is safe
fn is_safe_replacement(left: &[u8], right: &[u8]) -> bool {
    let is_substring = |needle: &[u8], haystack: &[u8]| {
        haystack
            .windows(needle.len())
            .any(|window| window == needle)
    };

    let is_substring_of_original = is_substring(left, right);
    let is_substring_of_replacement = KNOWN_NAMED_CHARSETS
        .iter()
        .any(|charset| is_substring(left, charset.bytes));

    is_substring_of_original == is_substring_of_replacement
}

/// FURB156
pub(crate) fn hardcoded_string_charset_comparison(checker: &mut Checker, compare: &ExprCompare) {
    let (
        [CmpOp::In | CmpOp::NotIn],
        [Expr::StringLiteral(string_literal @ ExprStringLiteral { value, .. })],
    ) = (compare.ops.as_ref(), compare.comparators.as_ref())
    else {
        return;
    };

    let right_bytes = value.to_str().as_bytes();

    let left_value = match compare.left.as_ref() {
        Expr::StringLiteral(ExprStringLiteral { value, .. }) => value.to_string(),
        _ => return, // If left side is not a string literal, we can't safely analyze it
    };

    let left_bytes = left_value.as_bytes();

    // Check if we can safely use set-like replacement
    let can_use_set_like = left_bytes.is_empty() || is_safe_replacement(left_bytes, right_bytes);

    if can_use_set_like {
        if let Some(charset) = check_charset_as_set(right_bytes) {
            push_diagnostic(checker, string_literal.range, charset);
        }
    } else {
        // For cases where set-like replacement isn't safe, only use exact matches
        if let Some(charset) = check_charset_exact(right_bytes) {
            push_diagnostic(checker, string_literal.range, charset);
        }
    }
}

/// FURB156
pub(crate) fn hardcoded_string_charset_literal(checker: &mut Checker, expr: &ExprStringLiteral) {
    if let Some(charset) = check_charset_exact(expr.value.to_str().as_bytes()) {
        push_diagnostic(checker, expr.range, charset);
    }
}
