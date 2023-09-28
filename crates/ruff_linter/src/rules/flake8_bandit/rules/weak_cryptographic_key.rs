use once_cell::sync::Lazy;
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Constant, Expr, ExprAttribute, ExprCall};
use ruff_text_size::{Ranged, TextRange};
use rustc_hash::FxHashSet;
use similar::DiffableStr;
use std::fmt::{Display, Formatter};

use crate::checkers::ast::Checker;

static VULNERABLE_ELLIPTIC_CURVE_KEYS: Lazy<FxHashSet<&'static str>> =
    Lazy::new(|| FxHashSet::from_iter(["SECP192R1", "SECT163K1", "SECT163R2"]));

#[derive(Debug, PartialEq, Eq)]
enum CryptographicKey {
    Dsa { key_size: u16 },
    Ec { algorithm: String },
    Rsa { key_size: u16 },
}

impl CryptographicKey {
    const fn minimum_key_size(&self) -> u16 {
        match self {
            Self::Dsa { .. } | Self::Rsa { .. } => 2048,
            Self::Ec { .. } => 224,
        }
    }

    fn is_vulnerable(&self) -> bool {
        match self {
            Self::Dsa { key_size } | Self::Rsa { key_size } => key_size < &self.minimum_key_size(),
            Self::Ec { algorithm } => VULNERABLE_ELLIPTIC_CURVE_KEYS.contains(algorithm.as_str()),
        }
    }
}

impl Display for CryptographicKey {
    fn fmt(&self, fmt: &mut Formatter) -> std::fmt::Result {
        match self {
            CryptographicKey::Dsa { .. } => fmt.write_str("DSA"),
            CryptographicKey::Ec { .. } => fmt.write_str("EC"),
            CryptographicKey::Rsa { .. } => fmt.write_str("RSA"),
        }
    }
}

/// ## What it does
/// Checks for uses of `cryptographic key lengths known to be vulnerable.
///
/// ## Why is this bad?
/// Small key lengths can easily be breakable, as computational power
/// increases. For DSA and RSA keys, it is recommended to use key lengths equal
/// or higher to 2048 bits. For EC, it is recommended to use curves higher than
/// 224 bits.
///
/// ## Example
/// ```python
/// from cryptography.hazmat.primitives.asymmetric import dsa, ec
///
/// dsa.generate_private_key(key_size=512)
/// ec.generate_private_key(curve=ec.SECT163K1)
/// ```
///
/// Use instead:
/// ```python
/// from cryptography.hazmat.primitives.asymmetric import dsa, ec
///
/// dsa.generate_private_key(key_size=4096)
/// ec.generate_private_key(curve=ec.SECP384R1)
/// ```
///
/// ## References
/// - [CSRC: Transitioning the Use of Cryptographic Algorithms and Key Lengths](https://csrc.nist.gov/pubs/sp/800/131/a/r2/final)
#[violation]
pub struct WeakCryptographicKey {
    cryptographic_key: CryptographicKey,
}

impl Violation for WeakCryptographicKey {
    #[derive_message_formats]
    fn message(&self) -> String {
        let WeakCryptographicKey { cryptographic_key } = self;
        let minimum_key_size = cryptographic_key.minimum_key_size();
        format!(
            "{cryptographic_key} key sizes below {minimum_key_size} bits are considered breakable"
        )
    }
}

fn extract_int_argument(call: &ExprCall, name: &str, position: usize) -> Option<(u16, TextRange)> {
    let Some(argument) = call.arguments.find_argument(name, position) else {
        return None;
    };
    let Expr::Constant(ast::ExprConstant {
        value: Constant::Int(i),
        ..
    }) = argument
    else {
        return None;
    };
    Some((i.as_u16()?, argument.range()))
}

fn extract_cryptographic_key(
    checker: &mut Checker,
    call: &ExprCall,
) -> Option<(CryptographicKey, TextRange)> {
    return checker
        .semantic()
        .resolve_call_path(&call.func)
        .and_then(|call_path| match call_path.as_slice() {
            ["cryptography", "hazmat", "primitives", "asymmetric", function, "generate_private_key"] => {
                return match function.as_str() {
                    Some("dsa") => {
                        let Some((key_size, range)) = extract_int_argument(call, "key_size", 0) else {return None};
                        return Some((CryptographicKey::Dsa { key_size }, range));
                    },
                    Some("rsa") => {
                        let Some((key_size, range)) = extract_int_argument(call, "key_size", 1) else {return None};
                        return Some((CryptographicKey::Rsa { key_size }, range));
                    },
                    Some("ec") => {
                        let Some(argument) = call.arguments.find_argument("curve", 0) else { return None };
                        let Expr::Attribute(ExprAttribute { attr, value, .. }) = argument else { return None };

                        if checker
                            .semantic()
                            .resolve_call_path(value)
                            .is_some_and(|call_path| matches!(call_path.as_slice(), ["cryptography", "hazmat", "primitives", "asymmetric", "ec"]))
                        {
                            return Some((CryptographicKey::Ec{algorithm: attr.as_str().to_string()}, argument.range()));
                        }
                        return None;
                    },
                    _ => None,
                };
            },
            ["Crypto" | "Cryptodome", "PublicKey", function, "generate"] => {
                return match function.as_str() {
                    Some("DSA") => {
                        let Some((key_size, range)) = extract_int_argument(call, "bits", 0) else {return None};
                        return Some((CryptographicKey::Dsa { key_size }, range));
                    },
                    Some("RSA") => {
                        let Some((key_size, range)) = extract_int_argument(call, "bits", 0) else {return None};
                        return Some((CryptographicKey::Dsa { key_size }, range));
                    },
                    _ => None,
                };
            },
            _ => None,
        });
}

/// S505
pub(crate) fn weak_cryptographic_key(checker: &mut Checker, call: &ExprCall) {
    let Expr::Attribute(_) = call.func.as_ref() else {
        return;
    };

    let Some((cryptographic_key, range)) = extract_cryptographic_key(checker, call) else {
        return;
    };

    if cryptographic_key.is_vulnerable() {
        checker.diagnostics.push(Diagnostic::new(
            WeakCryptographicKey { cryptographic_key },
            range,
        ));
    }
}
