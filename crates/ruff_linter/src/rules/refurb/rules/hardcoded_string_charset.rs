use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;
use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::parenthesize::parenthesized_range;
use ruff_python_ast::{CmpOp, Expr, ExprCall, ExprCompare, ExprStringLiteral};
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
    fn from_bytes(bytes: &[u8]) -> Option<Self> {
        bytes
            .iter()
            .try_fold(0, |acc, &byte| byte.is_ascii().then(|| acc | (1 << byte)))
            .map(Self)
    }

    /// Creates the set of ascii characters from `bytes`.
    /// Returns None if there is non-ascii byte.
    const fn from_bytes_const(bytes: &[u8]) -> Option<Self> {
        // TODO: remove in favor of [`Self::from_bytes`], when its implementation will be
        //  supported in `const` fn
        //  - https://github.com/rust-lang/rust/issues/67792
        //  - https://github.com/rust-lang/rust/issues/87575
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

    const fn from_bytes_const_unwrap(bytes: &[u8]) -> Self {
        // TODO: replace with .unwrap() in the caller, when Option::unwrap will be stable in `const fn`
        //  - https://github.com/rust-lang/rust/issues/67441)
        match Self::from_bytes_const(bytes) {
            Some(res) => res,
            None => unreachable!(),
        }
    }
}

impl NamedCharset {
    const fn new(name: &'static str, bytes: &'static [u8]) -> Self {
        Self {
            name,
            bytes,
            // SAFETY: The named charset is guaranteed to have only ascii bytes.
            ascii_char_set: AsciiCharSet::from_bytes_const_unwrap(bytes),
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

/// FURB156
pub(crate) fn hardcoded_string_charset_comparison(checker: &mut Checker, compare: &ExprCompare) {
    let (
        [CmpOp::In | CmpOp::NotIn],
        [Expr::StringLiteral(string_literal @ ExprStringLiteral { value, .. })],
    ) = (compare.ops.as_ref(), compare.comparators.as_ref())
    else {
        return;
    };

    let bytes = value.to_str().as_bytes();

    let Some(charset) = check_charset_as_set(bytes) else {
        return;
    };

    // In this case the diagnostic will be emitted via string_literal check.
    if charset.bytes == bytes {
        return;
    }

    let range = parenthesized_range(
        string_literal.into(),
        compare.into(),
        checker.comment_ranges(),
        checker.locator().contents(),
    )
    .unwrap_or(string_literal.range);

    push_diagnostic(checker, range, charset);
}

/// FURB156
pub(crate) fn hardcoded_string_charset_literal(checker: &mut Checker, expr: &ExprStringLiteral) {
    if let Some(charset) = check_charset_exact(expr.value.to_str().as_bytes()) {
        let range = parenthesized_range(
            expr.into(),
            checker.semantic().current_expression_parent().map_or_else(
                || checker.semantic().current_statement().into(),
                |parent| {
                    if let Expr::Call(ExprCall { arguments, .. }) = parent {
                        arguments.into()
                    } else {
                        parent.into()
                    }
                },
            ),
            checker.comment_ranges(),
            checker.locator().contents(),
        )
        .unwrap_or(expr.range);

        push_diagnostic(checker, range, charset);
    }
}
