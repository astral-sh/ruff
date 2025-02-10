use rustc_hash::{FxBuildHasher, FxHashSet};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{Expr, ExprCall, ExprStringLiteral};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for repeated keyword arguments in function calls.
///
/// ## Why is this bad?
/// Python does not allow repeated keyword arguments in function calls. If a
/// function is called with the same keyword argument multiple times, the
/// interpreter will raise an exception.
///
/// ## Example
/// ```python
/// func(1, 2, c=3, **{"c": 4})
/// ```
///
/// ## References
/// - [Python documentation: Argument](https://docs.python.org/3/glossary.html#term-argument)
#[derive(ViolationMetadata)]
pub(crate) struct RepeatedKeywordArgument {
    duplicate_keyword: String,
}

impl Violation for RepeatedKeywordArgument {
    #[derive_message_formats]
    fn message(&self) -> String {
        let Self { duplicate_keyword } = self;
        format!("Repeated keyword argument: `{duplicate_keyword}`")
    }
}

pub(crate) fn repeated_keyword_argument(checker: &Checker, call: &ExprCall) {
    let ExprCall { arguments, .. } = call;

    let mut seen = FxHashSet::with_capacity_and_hasher(arguments.keywords.len(), FxBuildHasher);

    for keyword in &*arguments.keywords {
        if let Some(id) = &keyword.arg {
            // Ex) `func(a=1, a=2)`
            if !seen.insert(id.as_str()) {
                checker.report_diagnostic(Diagnostic::new(
                    RepeatedKeywordArgument {
                        duplicate_keyword: id.to_string(),
                    },
                    keyword.range(),
                ));
            }
        } else if let Expr::Dict(dict) = &keyword.value {
            // Ex) `func(**{"a": 1, "a": 2})`
            for key in dict.iter_keys().flatten() {
                if let Expr::StringLiteral(ExprStringLiteral { value, .. }) = key {
                    if !seen.insert(value.to_str()) {
                        checker.report_diagnostic(Diagnostic::new(
                            RepeatedKeywordArgument {
                                duplicate_keyword: value.to_string(),
                            },
                            key.range(),
                        ));
                    }
                }
            }
        }
    }
}
