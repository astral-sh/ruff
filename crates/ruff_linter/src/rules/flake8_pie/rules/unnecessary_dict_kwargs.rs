use std::hash::BuildHasherDefault;

use itertools::Itertools;
use rustc_hash::FxHashSet;

use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr};
use ruff_python_stdlib::identifiers::is_identifier;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::fix::edits::{remove_argument, Parentheses};

/// ## What it does
/// Checks for unnecessary `dict` kwargs.
///
/// ## Why is this bad?
/// If the `dict` keys are valid identifiers, they can be passed as keyword
/// arguments directly.
///
/// ## Example
/// ```python
/// def foo(bar):
///     return bar + 1
///
///
/// print(foo(**{"bar": 2}))  # prints 3
/// ```
///
/// Use instead:
/// ```python
/// def foo(bar):
///     return bar + 1
///
///
/// print(foo(bar=2))  # prints 3
/// ```
///
/// ## References
/// - [Python documentation: Dictionary displays](https://docs.python.org/3/reference/expressions.html#dictionary-displays)
/// - [Python documentation: Calls](https://docs.python.org/3/reference/expressions.html#calls)
#[violation]
pub struct UnnecessaryDictKwargs;

impl Violation for UnnecessaryDictKwargs {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unnecessary `dict` kwargs")
    }

    fn fix_title(&self) -> Option<String> {
        Some(format!("Remove unnecessary kwargs"))
    }
}

/// PIE804
pub(crate) fn unnecessary_dict_kwargs(checker: &mut Checker, call: &ast::ExprCall) {
    let mut duplicate_keywords = None;
    for keyword in &call.arguments.keywords {
        // keyword is a spread operator (indicated by None).
        if keyword.arg.is_some() {
            continue;
        }

        let Expr::Dict(ast::ExprDict { keys, values, .. }) = &keyword.value else {
            continue;
        };

        // Ex) `foo(**{**bar})`
        if matches!(keys.as_slice(), [None]) {
            let mut diagnostic = Diagnostic::new(UnnecessaryDictKwargs, keyword.range());

            diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                format!("**{}", checker.locator().slice(values[0].range())),
                keyword.range(),
            )));

            checker.diagnostics.push(diagnostic);
            continue;
        }

        // Ensure that every keyword is a valid keyword argument (e.g., avoid errors for cases like
        // `foo(**{"bar-bar": 1})`).
        let kwargs = keys
            .iter()
            .filter_map(|key| key.as_ref().and_then(as_kwarg))
            .collect::<Vec<_>>();
        if kwargs.len() != keys.len() {
            continue;
        }

        let mut diagnostic = Diagnostic::new(UnnecessaryDictKwargs, keyword.range());

        if values.is_empty() {
            diagnostic.try_set_fix(|| {
                remove_argument(
                    keyword,
                    &call.arguments,
                    Parentheses::Preserve,
                    checker.locator().contents(),
                )
                .map(Fix::safe_edit)
            });
        } else {
            // Compute the set of duplicate keywords (lazily).
            if duplicate_keywords.is_none() {
                duplicate_keywords = Some(duplicates(call));
            }

            // Avoid fixing if doing so could introduce a duplicate keyword argument.
            if let Some(duplicate_keywords) = duplicate_keywords.as_ref() {
                if kwargs
                    .iter()
                    .all(|kwarg| !duplicate_keywords.contains(kwarg))
                {
                    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                        kwargs
                            .iter()
                            .zip(values.iter())
                            .map(|(kwarg, value)| {
                                format!("{}={}", kwarg, checker.locator().slice(value.range()))
                            })
                            .join(", "),
                        keyword.range(),
                    )));
                }
            }
        }

        checker.diagnostics.push(diagnostic);
    }
}

/// Determine the set of keywords that appear in multiple positions (either directly, as in
/// `func(x=1)`, or indirectly, as in `func(**{"x": 1})`).
fn duplicates(call: &ast::ExprCall) -> FxHashSet<&str> {
    let mut seen = FxHashSet::with_capacity_and_hasher(
        call.arguments.keywords.len(),
        BuildHasherDefault::default(),
    );
    let mut duplicates = FxHashSet::with_capacity_and_hasher(
        call.arguments.keywords.len(),
        BuildHasherDefault::default(),
    );
    for keyword in &call.arguments.keywords {
        if let Some(name) = &keyword.arg {
            if !seen.insert(name.as_str()) {
                duplicates.insert(name.as_str());
            }
        } else if let Expr::Dict(ast::ExprDict { keys, .. }) = &keyword.value {
            for key in keys {
                if let Some(name) = key.as_ref().and_then(as_kwarg) {
                    if !seen.insert(name) {
                        duplicates.insert(name);
                    }
                }
            }
        }
    }
    duplicates
}

/// Return `Some` if a key is a valid keyword argument name, or `None` otherwise.
fn as_kwarg(key: &Expr) -> Option<&str> {
    if let Expr::StringLiteral(ast::ExprStringLiteral { value, .. }) = key {
        if is_identifier(value.to_str()) {
            return Some(value.to_str());
        }
    }
    None
}
