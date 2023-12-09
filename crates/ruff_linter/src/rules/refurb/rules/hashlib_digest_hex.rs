use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{Expr, ExprAttribute, ExprCall};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for the use of `.digest().hex()`.
///
/// ## Why is this bad?
/// Use `.hexdigest()` to get a hex digest from a hash.
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
/// ## References
/// - [Python documentation: `hashlib` — Secure hashes and message digests](https://docs.python.org/3/library/hashlib.html)
#[violation]
pub struct HashlibDigestHex;

impl Violation for HashlibDigestHex {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use of hashlib `.digest().hex()`")
    }

    fn fix_title(&self) -> Option<String> {
        Some("Replace hashlib `.digest().hex()` with `.hexdigest()`".to_string())
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

    let Expr::Call(ExprCall {
        func, range: r3, ..
    }) = value.as_ref()
    else {
        return;
    };

    if !checker.semantic().resolve_call_path(func).is_some_and(
        |call_path: smallvec::SmallVec<[&str; 8]>| {
            matches!(
                call_path.as_slice(),
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
        },
    ) {
        return;
    }
    let mut diagnostic = Diagnostic::new(HashlibDigestHex, call.range());
    if arguments.is_empty() {
        diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
            ".hexdigest".to_string(),
            TextRange::new(r3.end(), call.func.end()),
        )));
    }

    checker.diagnostics.push(diagnostic);
}
