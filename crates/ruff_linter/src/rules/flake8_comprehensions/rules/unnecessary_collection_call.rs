use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast};
use ruff_text_size::{Ranged, TextSize};

use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::rules::flake8_comprehensions::fixes;
use crate::rules::flake8_comprehensions::fixes::{pad_end, pad_start};
use crate::rules::flake8_comprehensions::settings::Settings;
use crate::{AlwaysFixableViolation, Edit, Fix};

/// ## What it does
/// Checks for unnecessary `dict()`, `list()` or `tuple()` calls that can be
/// rewritten as empty literals.
///
/// ## Why is this bad?
/// It's unnecessary to call, e.g., `dict()` as opposed to using an empty
/// literal (`{}`). The former is slower because the name `dict` must be
/// looked up in the global scope in case it has been rebound.
///
/// ## Example
/// ```python
/// dict()
/// dict(a=1, b=2)
/// list()
/// tuple()
/// ```
///
/// Use instead:
/// ```python
/// {}
/// {"a": 1, "b": 2}
/// []
/// ()
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as unsafe, as it may occasionally drop comments
/// when rewriting the call. In most cases, though, comments will be preserved.
///
/// ## Options
/// - `lint.flake8-comprehensions.allow-dict-calls-with-keyword-arguments`
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.61", category = Category::Complexity)]
pub(crate) struct UnnecessaryCollectionCall {
    kind: Collection,
}

impl AlwaysFixableViolation for UnnecessaryCollectionCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnnecessaryCollectionCall { kind } = self;
        format!("Unnecessary `{kind}()` call (rewrite as a literal)")
    }

    fn fix_title(&self) -> String {
        "Rewrite as a literal".to_string()
    }
}

/// C408
pub(crate) fn unnecessary_collection_call(
    checker: &Checker,
    call_fields: &ast::ExprCallFields<'_>,
    settings: &Settings,
) {
    let call = call_fields.node;
    let empty = call_fields.arguments.is_empty();
    if !empty
        && (settings.allow_dict_calls_with_keyword_arguments
            || call_fields
                .arguments
                .iter_source_order()
                .any(|item| match item {
                    ast::ArgOrKeyword::Arg(_) => true,
                    ast::ArgOrKeyword::Keyword(keyword) => keyword.arg().is_none(),
                }))
    {
        return;
    }
    let Some(builtin) = checker.semantic().resolve_builtin_symbol(call_fields.func) else {
        return;
    };
    let collection = match builtin {
        "dict" => Collection::Dict,
        "list" if empty => Collection::List,
        "tuple" if empty => Collection::Tuple,
        _ => return,
    };

    let mut diagnostic = checker.report_diagnostic(
        UnnecessaryCollectionCall { kind: collection },
        call_fields.range,
    );

    // Convert `dict()` to `{}`.
    if empty {
        diagnostic.set_fix({
            // Replace from the start of the call to the start of the argument.
            let call_start = Edit::replacement(
                match collection {
                    Collection::Dict => pad_start(
                        "{",
                        call_fields.range,
                        checker.locator(),
                        checker.semantic(),
                    ),
                    Collection::List => "[".to_string(),
                    Collection::Tuple => "(".to_string(),
                },
                call.start(),
                call_fields.arguments.start() + TextSize::from(1),
            );

            // Replace from the end of the inner list or tuple to the end of the call with `}`.
            let call_end = Edit::replacement(
                match collection {
                    Collection::Dict => pad_end(
                        "}",
                        call_fields.range,
                        checker.locator(),
                        checker.semantic(),
                    ),
                    Collection::List => "]".to_string(),
                    Collection::Tuple => ")".to_string(),
                },
                call_fields.arguments.end() - TextSize::from(1),
                call.end(),
            );

            Fix::unsafe_edits(call_start, [call_end])
        });
    } else {
        // Convert `dict(a=1, b=2)` to `{"a": 1, "b": 2}`.
        diagnostic.try_set_fix(|| {
            fixes::fix_unnecessary_collection_call(call, checker).map(Fix::unsafe_edit)
        });
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum Collection {
    Tuple,
    List,
    Dict,
}

impl Collection {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Dict => "dict",
            Self::List => "list",
            Self::Tuple => "tuple",
        }
    }
}

impl std::fmt::Display for Collection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
