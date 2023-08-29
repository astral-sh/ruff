use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::is_const_false;
use ruff_python_ast::{self as ast, Arguments};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

use super::super::helpers::string_literal;

/// ## What it does
/// Checks for uses of weak or broken cryptographic hash functions.
///
/// ## Why is this bad?
/// Weak or broken cryptographic hash functions may be susceptible to
/// collision attacks (where two different inputs produce the same hash) or
/// pre-image attacks (where an attacker can find an input that produces a
/// given hash). This can lead to security vulnerabilities in applications
/// that rely on these hash functions.
///
/// Avoid using weak or broken cryptographic hash functions in security
/// contexts. Instead, use a known secure hash function such as SHA256.
///
/// ## Example
/// ```python
/// import hashlib
///
///
/// def certificate_is_valid(certificate: bytes, known_hash: str) -> bool:
///     hash = hashlib.md5(certificate).hexdigest()
///     return hash == known_hash
/// ```
///
/// Use instead:
/// ```python
/// import hashlib
///
///
/// def certificate_is_valid(certificate: bytes, known_hash: str) -> bool:
///     hash = hashlib.sha256(certificate).hexdigest()
///     return hash == known_hash
/// ```
///
/// ## References
/// - [Python documentation: `hashlib` — Secure hashes and message digests](https://docs.python.org/3/library/hashlib.html)
/// - [Common Weakness Enumeration: CWE-327](https://cwe.mitre.org/data/definitions/327.html)
/// - [Common Weakness Enumeration: CWE-328](https://cwe.mitre.org/data/definitions/328.html)
/// - [Common Weakness Enumeration: CWE-916](https://cwe.mitre.org/data/definitions/916.html)
#[violation]
pub struct HashlibInsecureHashFunction {
    string: String,
}

impl Violation for HashlibInsecureHashFunction {
    #[derive_message_formats]
    fn message(&self) -> String {
        let HashlibInsecureHashFunction { string } = self;
        format!("Probable use of insecure hash functions in `hashlib`: `{string}`")
    }
}

/// S324
pub(crate) fn hashlib_insecure_hash_functions(checker: &mut Checker, call: &ast::ExprCall) {
    if let Some(hashlib_call) =
        checker
            .semantic()
            .resolve_call_path(&call.func)
            .and_then(|call_path| match call_path.as_slice() {
                ["hashlib", "new"] => Some(HashlibCall::New),
                ["hashlib", "md4"] => Some(HashlibCall::WeakHash("md4")),
                ["hashlib", "md5"] => Some(HashlibCall::WeakHash("md5")),
                ["hashlib", "sha"] => Some(HashlibCall::WeakHash("sha")),
                ["hashlib", "sha1"] => Some(HashlibCall::WeakHash("sha1")),
                _ => None,
            })
    {
        if !is_used_for_security(&call.arguments) {
            return;
        }
        match hashlib_call {
            HashlibCall::New => {
                if let Some(name_arg) = call.arguments.find_argument("name", 0) {
                    if let Some(hash_func_name) = string_literal(name_arg) {
                        // `hashlib.new` accepts both lowercase and uppercase names for hash
                        // functions.
                        if matches!(
                            hash_func_name,
                            "md4" | "md5" | "sha" | "sha1" | "MD4" | "MD5" | "SHA" | "SHA1"
                        ) {
                            checker.diagnostics.push(Diagnostic::new(
                                HashlibInsecureHashFunction {
                                    string: hash_func_name.to_string(),
                                },
                                name_arg.range(),
                            ));
                        }
                    }
                }
            }
            HashlibCall::WeakHash(func_name) => {
                checker.diagnostics.push(Diagnostic::new(
                    HashlibInsecureHashFunction {
                        string: (*func_name).to_string(),
                    },
                    call.func.range(),
                ));
            }
        }
    }
}

fn is_used_for_security(arguments: &Arguments) -> bool {
    arguments
        .find_keyword("usedforsecurity")
        .map_or(true, |keyword| !is_const_false(&keyword.value))
}

#[derive(Debug)]
enum HashlibCall {
    New,
    WeakHash(&'static str),
}
