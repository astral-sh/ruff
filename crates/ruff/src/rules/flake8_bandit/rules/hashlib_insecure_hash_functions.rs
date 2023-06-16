use rustpython_parser::ast::{Expr, Keyword, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::{is_const_false, SimpleCallArgs};

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

/// S324
pub(crate) fn hashlib_insecure_hash_functions(
    checker: &mut Checker,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) {
    if let Some(hashlib_call) = checker
        .semantic()
        .resolve_call_path(func)
        .and_then(|call_path| match call_path.as_slice() {
            ["hashlib", "new"] => Some(HashlibCall::New),
            ["hashlib", "md4"] => Some(HashlibCall::WeakHash("md4")),
            ["hashlib", "md5"] => Some(HashlibCall::WeakHash("md5")),
            ["hashlib", "sha"] => Some(HashlibCall::WeakHash("sha")),
            ["hashlib", "sha1"] => Some(HashlibCall::WeakHash("sha1")),
            _ => None,
        })
    {
        match hashlib_call {
            HashlibCall::New => {
                let call_args = SimpleCallArgs::new(args, keywords);

                if !is_used_for_security(&call_args) {
                    return;
                }

                if let Some(name_arg) = call_args.argument("name", 0) {
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

fn is_used_for_security(call_args: &SimpleCallArgs) -> bool {
    match call_args.keyword_argument("usedforsecurity") {
        Some(expr) => !is_const_false(expr),
        _ => true,
    }
}

enum HashlibCall {
    New,
    WeakHash(&'static str),
}
