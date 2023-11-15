use std::collections::HashSet;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{Expr, ExprDict, ExprStringLiteral, Keyword};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for repeated keyword arguments passed to a function call
///
/// ## Why is this bad?
/// Python does not allow for multiple values to be assigned to the same
/// keyword argument in a single function call.
///
/// ## Example
/// ```python
/// func(1, 2, c=3, **{"c": 4})
/// ```
///
/// Use instead:
/// ```python
/// func(1, 2, **{"c": 4})
/// ```
///
/// ## References
/// - [Python documentation: Argument](https://docs.python.org/3/glossary.html#term-argument)
#[violation]
pub struct RepeatedKeywords {
    duplicate_keyword: String,
}

impl Violation for RepeatedKeywords {
    #[derive_message_formats]
    fn message(&self) -> String {
        let dupe = &self.duplicate_keyword;
        format!("Repeated keyword argument: `{dupe}`")
    }
}

type KeywordRecordFn<'a> = Box<dyn FnMut(&str, TextRange) + 'a>;

fn generate_record_func<'a>(checker: &'a mut Checker) -> KeywordRecordFn<'a> {
    // init some hash sets to be captured by the closure
    let mut seen = HashSet::<String>::new();
    let mut dupes = HashSet::<String>::new();

    let inner = move |keyword: &str, range| {
        // Add an error the first time we see the duplicate
        if seen.contains(keyword) && !dupes.contains(keyword) {
            dupes.insert(String::from(keyword));
            checker.diagnostics.push(Diagnostic::new(
                RepeatedKeywords {
                    duplicate_keyword: keyword.into(),
                },
                range,
            ));
        } else {
            seen.insert(String::from(keyword));
        }
    };

    Box::new(inner)
}

pub(crate) fn repeated_keywords(checker: &mut Checker, keywords: &Vec<Keyword>) {
    let mut record_keyword = generate_record_func(checker);

    for keyword in keywords {
        if let Some(id) = &keyword.arg {
            record_keyword(id.as_str(), keyword.range());
        } else if let Expr::Dict(ExprDict {
            // We only want to check dict keys if there is NO arg associated with them
            keys,
            range: _,
            values: _,
        }) = &keyword.value
        {
            for key in keys.iter().flatten() {
                if let Expr::StringLiteral(ExprStringLiteral {
                    value,
                    range: _,
                    unicode: _,
                    implicit_concatenated: _,
                }) = key
                {
                    record_keyword(value, key.range());
                }
            }
        }
    }
}
