use rustc_hash::FxHashSet;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, ExprKind};
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
pub(crate) fn repeated_keyword_argument(checker: &Checker, call: &ast::ExprCallFields<'_>) {
    let arguments = call.arguments;

    let mut unpacked_dicts = arguments
        .keywords()
        .iter()
        .filter_map(|keyword| {
            if keyword.is_variadic() {
                keyword.value().as_dict_expr()
            } else {
                None
            }
        })
        .peekable();
    if unpacked_dicts.peek().is_none() {
        return;
    }

    // Explicit names anywhere in the call take precedence over names in unpacked dicts.
    let mut seen: FxHashSet<_> = arguments
        .keywords()
        .iter()
        .filter_map(|keyword| keyword.arg().map(ruff_python_ast::Identifier::as_str))
        .collect();
    for dict in unpacked_dicts {
        for key in dict.iter_keys().flatten() {
            if let ExprKind::StringLiteral(string_literal) = key.classify() {
                let value = string_literal.value();
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
