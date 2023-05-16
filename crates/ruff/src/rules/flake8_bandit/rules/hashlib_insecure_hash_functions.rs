use rustpython_parser::ast::{self, Constant, Expr, ExprKind, Keyword};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::SimpleCallArgs;

use crate::checkers::ast::Checker;

use super::super::helpers::string_literal;

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

const WEAK_HASHES: [&str; 4] = ["md4", "md5", "sha", "sha1"];

fn is_used_for_security(call_args: &SimpleCallArgs) -> bool {
    match call_args.keyword_argument("usedforsecurity") {
        Some(expr) => !matches!(
            &expr.node,
            ExprKind::Constant(ast::ExprConstant {
                value: Constant::Bool(false),
                ..
            })
        ),
        _ => true,
    }
}

enum HashlibCall {
    New,
    WeakHash(&'static str),
}

/// S324
pub(crate) fn hashlib_insecure_hash_functions(
    checker: &mut Checker,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) {
    if let Some(hashlib_call) = checker.ctx.resolve_call_path(func).and_then(|call_path| {
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

                if let Some(name_arg) = call_args.argument("name", 0) {
                    if let Some(hash_func_name) = string_literal(name_arg) {
                        if WEAK_HASHES.contains(&hash_func_name.to_lowercase().as_str()) {
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
                let call_args = SimpleCallArgs::new(args, keywords);

                if !is_used_for_security(&call_args) {
                    return;
                }

                checker.diagnostics.push(Diagnostic::new(
                    HashlibInsecureHashFunction {
                        string: (*func_name).to_string(),
                    },
                    func.range(),
                ));
            }
        }
    }
}
