use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Constant, Expr, ExprKind, Keyword};

use super::super::helpers::string_literal;
use crate::ast::helpers::SimpleCallArgs;
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    pub struct HashlibInsecureHashFunction {
        pub string: String,
    }
);
impl Violation for HashlibInsecureHashFunction {
    #[derive_message_formats]
    fn message(&self) -> String {
        let HashlibInsecureHashFunction { string } = self;
        format!(
            "Probable use of insecure hash functions in `hashlib`: \"{}\"",
            string.escape_debug()
        )
    }
}

const WEAK_HASHES: [&str; 4] = ["md4", "md5", "sha", "sha1"];

fn is_used_for_security(call_args: &SimpleCallArgs) -> bool {
    match call_args.get_argument("usedforsecurity", None) {
        Some(expr) => !matches!(
            &expr.node,
            ExprKind::Constant {
                value: Constant::Bool(false),
                ..
            }
        ),
        _ => true,
    }
}

enum HashlibCall {
    New,
    WeakHash(&'static str),
}

/// S324
pub fn hashlib_insecure_hash_functions(
    checker: &mut Checker,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) {
    if let Some(hashlib_call) = checker.resolve_call_path(func).and_then(|call_path| {
        if call_path.as_slice() == ["hashlib", "new"] {
            Some(HashlibCall::New)
        } else {
            WEAK_HASHES
                .iter()
                .find(|hash| call_path.as_slice() == ["hashlib", hash])
                .map(|hash| HashlibCall::WeakHash(hash))
        }
    }) {
        match hashlib_call {
            HashlibCall::New => {
                let call_args = SimpleCallArgs::new(args, keywords);

                if !is_used_for_security(&call_args) {
                    return;
                }

                if let Some(name_arg) = call_args.get_argument("name", Some(0)) {
                    if let Some(hash_func_name) = string_literal(name_arg) {
                        if WEAK_HASHES.contains(&hash_func_name.to_lowercase().as_str()) {
                            checker.diagnostics.push(Diagnostic::new(
                                HashlibInsecureHashFunction {
                                    string: hash_func_name.to_string(),
                                },
                                Range::from_located(name_arg),
                            ));
                        }
                    }
                }
            }
            HashlibCall::WeakHash(func_name) => {
                let call_args = SimpleCallArgs::new(args, keywords);

                if !is_used_for_security(&call_args) {
                    return;
                }

                checker.diagnostics.push(Diagnostic::new(
                    HashlibInsecureHashFunction {
                        string: (*func_name).to_string(),
                    },
                    Range::from_located(func),
                ));
            }
        }
    }
}
