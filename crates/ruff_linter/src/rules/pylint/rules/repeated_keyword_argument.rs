use rustc_hash::{FxBuildHasher, FxHashSet};

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{Expr, ExprCall, ExprStringLiteral};
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;

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
#[violation_metadata(stable_since = "0.5.0", category = Category::Correctness)]
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

/// PLE1132
pub(crate) fn repeated_keyword_argument(checker: &Checker, call: &ExprCall) {
    let ExprCall { arguments, .. } = call;

    // Avoid allocating if there's only one non-unpacked keyword argument, or the unpacked value is
    // not a dict literal.
    if let [keyword] = &*arguments.keywords {
        if keyword.arg.is_some() || !keyword.value.is_dict_expr() {
            return;
        }
    }

    let mut seen = FxHashSet::with_capacity_and_hasher(arguments.keywords.len(), FxBuildHasher);

    for keyword in &*arguments.keywords {
        if let Some(id) = &keyword.arg {
            seen.insert(id.as_str());
        }
    }

    for keyword in &*arguments.keywords {
        if keyword.arg.is_none()
            && let Expr::Dict(dict) = &keyword.value
        {
            for key in dict.iter_keys().flatten() {
                if let Expr::StringLiteral(ExprStringLiteral { value, .. }) = key {
                    if !seen.insert(value.to_str()) {
                        checker.report_diagnostic(
                            RepeatedKeywordArgument {
                                duplicate_keyword: value.to_string(),
                            },
                            key.range(),
                        );
                    }
                }
            }
        }
    }
}
