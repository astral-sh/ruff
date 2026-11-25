use ruff_diagnostics::Applicability;
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{Expr, ExprAttribute, ExprCall, token::parenthesized_range};
use ruff_python_semantic::Modules;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::{Edit, Fix, FixAvailability, Violation};

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
/// This rule's fix is marked as safe, unless the expression contains comments.
///
/// ## References
/// - [Python documentation: `hashlib`](https://docs.python.org/3/library/hashlib.html)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "0.5.0", category = Category::Complexity)]
pub(crate) struct HashlibDigestHex;

impl Violation for HashlibDigestHex {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Use of hashlib's `.digest().hex()`".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Replace with `.hexdigest()`".to_string())
    }
}

/// FURB181
pub(crate) fn hashlib_digest_hex(checker: &Checker, call: &ExprCall) {
    if !checker.semantic().seen_module(Modules::HASHLIB) {
        return;
    }

    if !call.arguments.is_empty() {
        return;
    }

    let Expr::Attribute(ExprAttribute {
        attr,
        value: digest_call_expr,
        ..
    }) = call.func.as_ref()
    else {
        return;
    };

    if attr.as_str() != "hex" {
        return;
    }

    let Expr::Call(
        digest_call @ ExprCall {
            func: digest_func_expr,
            arguments,
            ..
        },
    ) = digest_call_expr.as_ref()
    else {
        return;
    };

    let Expr::Attribute(ExprAttribute {
        attr: digest_attr_id,
        value: hash_call_expr,
        ..
    }) = digest_func_expr.as_ref()
    else {
        return;
    };

    if digest_attr_id.as_str() != "digest" {
        return;
    }

    let Expr::Call(ExprCall { func, .. }) = hash_call_expr.as_ref() else {
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
                ]
            )
        })
    {
        let mut diagnostic = checker.report_diagnostic(HashlibDigestHex, call.range());
        if arguments.is_empty() {
            let tokens = checker.tokens();
            let hash_end = parenthesized_range(
                hash_call_expr.as_ref().into(),
                digest_func_expr.as_ref().into(),
                tokens,
            )
            .unwrap_or(hash_call_expr.range())
            .end();

            let digest_attr_paren =
                parenthesized_range(digest_func_expr.as_ref().into(), digest_call.into(), tokens);
            let digest_call_paren = parenthesized_range(
                digest_call_expr.as_ref().into(),
                call.func.as_ref().into(),
                tokens,
            );
            let hex_attr_paren =
                parenthesized_range(call.func.as_ref().into(), call.into(), tokens);

            let fix = if digest_attr_paren.is_none()
                && digest_call_paren.is_none()
                && hex_attr_paren.is_none()
            {
                let replacement_range = TextRange::new(hash_end, call.func.end());
                let applicability = if checker.comment_ranges().intersects(replacement_range) {
                    Applicability::Unsafe
                } else {
                    Applicability::Safe
                };
                Fix::applicable_edit(
                    Edit::range_replacement(".hexdigest".to_string(), replacement_range),
                    applicability,
                )
            } else {
                let digest_call_range = digest_call_paren.unwrap_or(digest_call.range());
                let digest_edit =
                    Edit::range_replacement("hexdigest".to_string(), digest_attr_id.range());
                let suffix_edit =
                    Edit::range_deletion(TextRange::new(digest_call_range.end(), call.end()));
                let applicability = if checker
                    .comment_ranges()
                    .intersects(TextRange::new(hash_end, call.end()))
                {
                    Applicability::Unsafe
                } else {
                    Applicability::Safe
                };

                if let Some(hex_paren) = hex_attr_paren {
                    let prefix_edit = Edit::range_deletion(TextRange::new(
                        hex_paren.start(),
                        digest_call_range.start(),
                    ));
                    Fix::applicable_edits(prefix_edit, [digest_edit, suffix_edit], applicability)
                } else {
                    Fix::applicable_edits(digest_edit, [suffix_edit], applicability)
                }
            };

            diagnostic.set_fix(fix);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::codes::Rule;
    use crate::settings::LinterSettings;
    use crate::source_kind::SourceKind;
    use crate::test::test_contents;

    #[test]
    fn fixes_all_parenthesized_combinations() {
        let settings = LinterSettings::for_rule(Rule::HashlibDigestHex);

        for mask in 0u8..64 {
            let wrap = |bit: u8, s: String| -> String {
                if mask & (1 << bit) != 0 {
                    format!("({s})")
                } else {
                    s
                }
            };

            let mut expr = wrap(0, "hashlib.sha256".to_string());
            expr = wrap(1, format!("{expr}()"));
            expr = wrap(2, format!("{expr}.digest"));
            expr = wrap(3, format!("{expr}()"));
            expr = wrap(4, format!("{expr}.hex"));
            expr = wrap(5, format!("{expr}()"));

            let source = format!("import hashlib\ndigest = {expr}\n");
            let input = SourceKind::Python {
                code: source.clone(),
                is_stub: false,
            };
            let (diagnostics, fixed) = test_contents(&input, Path::new("test.py"), &settings);
            let output = fixed.source_code();
            assert_eq!(
                diagnostics.len(),
                1,
                "expected 1 diagnostic for mask {mask:#08b}: {source}"
            );
            assert_ne!(
                output, source,
                "fix did not apply for mask {mask:#08b}: {source}"
            );
            assert!(
                output.contains("hexdigest"),
                "expected hexdigest in output for mask {mask:#08b}: `{output}`"
            );
            ruff_python_parser::parse_module(output).unwrap_or_else(|err| {
                panic!("invalid Python after fix for mask {mask:#08b}: `{output}` ({err})")
            });
        }
    }
}
