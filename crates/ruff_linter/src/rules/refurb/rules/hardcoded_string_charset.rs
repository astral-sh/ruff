use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;
use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::ExprStringLiteral;
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
#[derive(ViolationMetadata)]
pub(crate) struct HardcodedStringCharset {
    name: &'static str,
}

impl AlwaysFixableViolation for HardcodedStringCharset {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Use of hardcoded string charset".to_string()
    }

    fn fix_title(&self) -> String {
        let HardcodedStringCharset { name } = self;
        format!("Replace hardcoded charset with `string.{name}`")
    }
}

/// FURB156
pub(crate) fn hardcoded_string_charset_literal(checker: &Checker, expr: &ExprStringLiteral) {
    // if the string literal is a docstring, the rule is not applied
    if checker.semantic().in_pep_257_docstring() {
        return;
    }

    if let Some(charset) = check_charset_exact(expr.value.to_str().as_bytes()) {
        push_diagnostic(checker, expr.range, charset);
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
struct NamedCharset {
    name: &'static str,
    bytes: &'static [u8],
    ascii_char_set: AsciiCharSet,
}

/// Represents the set of ascii characters in form of a bitset.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
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

fn check_charset_exact(bytes: &[u8]) -> Option<&NamedCharset> {
    KNOWN_NAMED_CHARSETS
        .iter()
        .find(|&charset| charset.bytes == bytes)
}

fn push_diagnostic(checker: &Checker, range: TextRange, charset: &NamedCharset) {
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
    checker.report_diagnostic(diagnostic);
}
