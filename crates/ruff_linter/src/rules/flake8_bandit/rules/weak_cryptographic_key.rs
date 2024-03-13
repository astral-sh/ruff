use std::fmt::{Display, Formatter};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr, ExprAttribute, ExprCall};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for uses of cryptographic keys with vulnerable key sizes.
///
/// ## Why is this bad?
/// Small keys are easily breakable. For DSA and RSA, keys should be at least
/// 2048 bits long. For EC, keys should be at least 224 bits long.
///
/// ## Example
/// ```python
/// from cryptography.hazmat.primitives.asymmetric import dsa, ec
///
/// dsa.generate_private_key(key_size=512)
/// ec.generate_private_key(curve=ec.SECT163K1())
/// ```
///
/// Use instead:
/// ```python
/// from cryptography.hazmat.primitives.asymmetric import dsa, ec
///
/// dsa.generate_private_key(key_size=4096)
/// ec.generate_private_key(curve=ec.SECP384R1())
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

/// S505
pub(crate) fn weak_cryptographic_key(checker: &mut Checker, call: &ExprCall) {
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
            Self::Ec { algorithm } => {
                matches!(algorithm.as_str(), "SECP192R1" | "SECT163K1" | "SECT163R2")
            }
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

fn extract_cryptographic_key(
    checker: &mut Checker,
    call: &ExprCall,
) -> Option<(CryptographicKey, TextRange)> {
    let qualified_name = checker.semantic().resolve_qualified_name(&call.func)?;
    match qualified_name.segments() {
        ["cryptography", "hazmat", "primitives", "asymmetric", function, "generate_private_key"] => {
            match *function {
                "dsa" => {
                    let (key_size, range) = extract_int_argument(call, "key_size", 0)?;
                    Some((CryptographicKey::Dsa { key_size }, range))
                }
                "rsa" => {
                    let (key_size, range) = extract_int_argument(call, "key_size", 1)?;
                    Some((CryptographicKey::Rsa { key_size }, range))
                }
                "ec" => {
                    let argument = call.arguments.find_argument("curve", 0)?;
                    let ExprAttribute { attr, value, .. } = argument.as_attribute_expr()?;
                    let qualified_name = checker.semantic().resolve_qualified_name(value)?;
                    if matches!(
                        qualified_name.segments(),
                        ["cryptography", "hazmat", "primitives", "asymmetric", "ec"]
                    ) {
                        Some((
                            CryptographicKey::Ec {
                                algorithm: attr.to_string(),
                            },
                            argument.range(),
                        ))
                    } else {
                        None
                    }
                }
                _ => None,
            }
        }
        ["Crypto" | "Cryptodome", "PublicKey", function, "generate"] => match *function {
            "DSA" => {
                let (key_size, range) = extract_int_argument(call, "bits", 0)?;
                Some((CryptographicKey::Dsa { key_size }, range))
            }
            "RSA" => {
                let (key_size, range) = extract_int_argument(call, "bits", 0)?;
                Some((CryptographicKey::Dsa { key_size }, range))
            }
            _ => None,
        },
        _ => None,
    }
}

fn extract_int_argument(call: &ExprCall, name: &str, position: usize) -> Option<(u16, TextRange)> {
    let argument = call.arguments.find_argument(name, position)?;
    let Expr::NumberLiteral(ast::ExprNumberLiteral {
        value: ast::Number::Int(i),
        ..
    }) = argument
    else {
        return None;
    };
    Some((i.as_u16()?, argument.range()))
}
