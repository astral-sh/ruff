use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::ExprStringLiteral;
use ruff_text_size::TextRange;

use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;
use crate::{AlwaysFixableViolation, Edit, Fix};

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
#[violation_metadata(preview_since = "0.7.0")]
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
}

impl NamedCharset {
    const fn new(name: &'static str, bytes: &'static [u8]) -> Self {
        Self { name, bytes }
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
    let mut diagnostic = checker.report_diagnostic(HardcodedStringCharset { name }, range);
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
}
