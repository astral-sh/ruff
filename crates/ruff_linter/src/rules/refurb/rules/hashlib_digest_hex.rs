use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{Expr, ExprAttribute, ExprCall};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for the use of `.digest().hex()` on a hashlib hash, like `sha512`.
///
/// ## Why is this bad?
/// When generating a hex digest from a hash, it's preferable to use the
/// `.hexdigest()` method, rather than calling `.digest()` and then `.hex()`,
/// as the former is more concise and readable.
///
/// ## Example
/// ```python
/// from hashlib import sha512
///
/// hashed = sha512(b"some data").digest().hex()
/// ```
///
/// Use instead:
/// ```python
/// from hashlib import sha512
///
/// hashed = sha512(b"some data").hexdigest()
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as unsafe, as the target of the `.digest()` call
/// could be a user-defined class that implements a `.hex()` method, rather
/// than a hashlib hash object.
///
/// ## References
/// - [Python documentation: `hashlib`](https://docs.python.org/3/library/hashlib.html)
#[violation]
pub struct HashlibDigestHex;

impl Violation for HashlibDigestHex {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use of hashlib's `.digest().hex()`")
    }

    fn fix_title(&self) -> Option<String> {
        Some("Replace with `.hexdigest()`".to_string())
    }
}

/// FURB181
pub(crate) fn hashlib_digest_hex(checker: &mut Checker, call: &ExprCall) {
    if !call.arguments.is_empty() {
        return;
    }

    let Expr::Attribute(ExprAttribute { attr, value, .. }) = call.func.as_ref() else {
        return;
    };

    if attr.as_str() != "hex" {
        return;
    }

    let Expr::Call(ExprCall {
        func, arguments, ..
    }) = value.as_ref()
    else {
        return;
    };

    let Expr::Attribute(ExprAttribute { attr, value, .. }) = func.as_ref() else {
        return;
    };

    if attr.as_str() != "digest" {
        return;
    }

    let Expr::Call(ExprCall { func, .. }) = value.as_ref() else {
        return;
    };

    if checker
        .semantic()
        .resolve_qualified_name(func)
        .is_some_and(|qualified_name| {
            matches!(
                qualified_name.segments(),
                [
                    "hashlib",
                    "md5"
                        | "sha1"
                        | "sha224"
                        | "sha256"
                        | "sha384"
                        | "sha512"
                        | "blake2b"
                        | "blake2s"
                        | "sha3_224"
                        | "sha3_256"
                        | "sha3_384"
                        | "sha3_512"
                        | "shake_128"
                        | "shake_256"
                        | "_Hash"
                ]
            )
        })
    {
        let mut diagnostic = Diagnostic::new(HashlibDigestHex, call.range());
        if arguments.is_empty() {
            diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
                ".hexdigest".to_string(),
                TextRange::new(value.end(), call.func.end()),
            )));
        }
        checker.diagnostics.push(diagnostic);
    }
}
