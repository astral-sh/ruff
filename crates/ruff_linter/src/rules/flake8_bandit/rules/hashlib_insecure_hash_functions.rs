use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::is_const_false;
use ruff_python_ast::{self as ast, Arguments};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

use super::super::helpers::string_literal;

/// ## What it does
/// Checks for uses of weak or broken cryptographic hash functions in
/// `hashlib` and `crypt` libraries.
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
/// - [Python documentation: `crypt` — Function to check Unix passwords](https://docs.python.org/3/library/crypt.html)
/// - [Common Weakness Enumeration: CWE-327](https://cwe.mitre.org/data/definitions/327.html)
/// - [Common Weakness Enumeration: CWE-328](https://cwe.mitre.org/data/definitions/328.html)
/// - [Common Weakness Enumeration: CWE-916](https://cwe.mitre.org/data/definitions/916.html)
#[violation]
pub struct HashlibInsecureHashFunction {
    library: String,
    string: String,
}

impl Violation for HashlibInsecureHashFunction {
    #[derive_message_formats]
    fn message(&self) -> String {
        let HashlibInsecureHashFunction { library, string } = self;
        format!("Probable use of insecure hash functions in `{library}`: `{string}`")
    }
}

/// S324
pub(crate) fn hashlib_insecure_hash_functions(checker: &mut Checker, call: &ast::ExprCall) {
    if let Some(weak_hash_call) = checker
        .semantic()
        .resolve_qualified_name(&call.func)
        .and_then(|qualified_name| match qualified_name.segments() {
            ["hashlib", "new"] => Some(WeakHashCall::Hashlib {
                call: HashlibCall::New,
            }),
            ["hashlib", "md4"] => Some(WeakHashCall::Hashlib {
                call: HashlibCall::WeakHash("md4"),
            }),
            ["hashlib", "md5"] => Some(WeakHashCall::Hashlib {
                call: HashlibCall::WeakHash("md5"),
            }),
            ["hashlib", "sha"] => Some(WeakHashCall::Hashlib {
                call: HashlibCall::WeakHash("sha"),
            }),
            ["hashlib", "sha1"] => Some(WeakHashCall::Hashlib {
                call: HashlibCall::WeakHash("sha1"),
            }),
            ["crypt", "crypt" | "mksalt"] => Some(WeakHashCall::Crypt),
            _ => None,
        })
    {
        match weak_hash_call {
            WeakHashCall::Hashlib { call: hashlib_call } => {
                detect_insecure_hashlib_calls(checker, call, hashlib_call);
            }
            WeakHashCall::Crypt => detect_insecure_crypt_calls(checker, call),
        }
    }
}

fn detect_insecure_hashlib_calls(
    checker: &mut Checker,
    call: &ast::ExprCall,
    hashlib_call: HashlibCall,
) {
    if !is_used_for_security(&call.arguments) {
        return;
    }

    match hashlib_call {
        HashlibCall::New => {
            let Some(name_arg) = call.arguments.find_argument("name", 0) else {
                return;
            };
            let Some(hash_func_name) = string_literal(name_arg) else {
                return;
            };

            // `hashlib.new` accepts both lowercase and uppercase names for hash
            // functions.
            if matches!(
                hash_func_name,
                "md4" | "md5" | "sha" | "sha1" | "MD4" | "MD5" | "SHA" | "SHA1"
            ) {
                checker.diagnostics.push(Diagnostic::new(
                    HashlibInsecureHashFunction {
                        library: "hashlib".to_string(),
                        string: hash_func_name.to_string(),
                    },
                    name_arg.range(),
                ));
            }
        }
        HashlibCall::WeakHash(func_name) => {
            checker.diagnostics.push(Diagnostic::new(
                HashlibInsecureHashFunction {
                    library: "hashlib".to_string(),
                    string: (*func_name).to_string(),
                },
                call.func.range(),
            ));
        }
    }
}

fn detect_insecure_crypt_calls(checker: &mut Checker, call: &ast::ExprCall) {
    let Some(method) = checker
        .semantic()
        .resolve_qualified_name(&call.func)
        .and_then(|qualified_name| match qualified_name.segments() {
            ["crypt", "crypt"] => Some(("salt", 1)),
            ["crypt", "mksalt"] => Some(("method", 0)),
            _ => None,
        })
        .and_then(|(argument_name, position)| {
            call.arguments.find_argument(argument_name, position)
        })
    else {
        return;
    };

    let Some(qualified_name) = checker.semantic().resolve_qualified_name(method) else {
        return;
    };

    if matches!(
        qualified_name.segments(),
        ["crypt", "METHOD_CRYPT" | "METHOD_MD5" | "METHOD_BLOWFISH"]
    ) {
        checker.diagnostics.push(Diagnostic::new(
            HashlibInsecureHashFunction {
                library: "crypt".to_string(),
                string: qualified_name.to_string(),
            },
            method.range(),
        ));
    }
}

fn is_used_for_security(arguments: &Arguments) -> bool {
    arguments
        .find_keyword("usedforsecurity")
        .map_or(true, |keyword| !is_const_false(&keyword.value))
}

#[derive(Debug, Copy, Clone)]
enum WeakHashCall {
    Hashlib { call: HashlibCall },
    Crypt,
}

#[derive(Debug, Copy, Clone)]
enum HashlibCall {
    New,
    WeakHash(&'static str),
}
