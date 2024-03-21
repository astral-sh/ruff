use anyhow::Result;

use ast::Keyword;
use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::is_constant;
use ruff_python_ast::{self as ast, Expr};
use ruff_source_file::Locator;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::fix::edits::{remove_argument, Parentheses};
use crate::fix::snippet::SourceCodeSnippet;

/// ## What it does
/// Checks for incorrect usages of `default_factory` as a keyword argument when
/// initializing a `defaultdict`.
///
/// ## Why is this bad?
/// The `defaultdict` constructor accepts a callable as its first argument.
/// For example, it's common to initialize a `defaultdict` with `int` or `list`
/// via `defaultdict(int)` or `defaultdict(list)`, to create a dictionary that
/// returns `0` or `[]` respectively when a key is missing.
///
/// The default factory _must_ be provided as a positional argument, as all
/// keyword arguments to `defaultdict` are interpreted as initial entries in
/// the dictionary. For example, `defaultdict(foo=1, bar=2)` will create a
/// dictionary with `{"foo": 1, "bar": 2}` as its initial entries.
///
/// As such, `defaultdict(default_factory=list)` will create a dictionary with
/// `{"default_factory": list}` as its initial entry, instead of a dictionary
/// that returns `[]` when a key is missing. Specifying a `default_factory`
/// keyword argument is almost always a mistake, and one that type checkers
/// can't reliably detect.
///
/// ## Fix safety
/// This rule's fix is marked as unsafe, as converting `default_factory` from a
/// keyword to a positional argument will change the behavior of the code, even
/// if the keyword argument was used erroneously.
///
/// ## Examples
/// ```python
/// defaultdict(default_factory=int)
/// defaultdict(default_factory=list)
/// ```
///
/// Use instead:
/// ```python
/// defaultdict(int)
/// defaultdict(list)
/// ```
#[violation]
pub struct DefaultFactoryKwarg {
    default_factory: SourceCodeSnippet,
}

impl Violation for DefaultFactoryKwarg {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`default_factory` is a positional-only argument to `defaultdict`")
    }

    fn fix_title(&self) -> Option<String> {
        let DefaultFactoryKwarg { default_factory } = self;
        if let Some(default_factory) = default_factory.full_display() {
            Some(format!("Replace with `defaultdict({default_factory})`"))
        } else {
            Some("Use positional argument".to_string())
        }
    }
}

/// RUF026
pub(crate) fn default_factory_kwarg(checker: &mut Checker, call: &ast::ExprCall) {
    // If the call isn't a `defaultdict` constructor, return.
    if !checker
        .semantic()
        .resolve_qualified_name(call.func.as_ref())
        .is_some_and(|qualified_name| {
            matches!(qualified_name.segments(), ["collections", "defaultdict"])
        })
    {
        return;
    }

    // If the user provided a positional argument for `default_factory`, return.
    if !call.arguments.args.is_empty() {
        return;
    }

    // If the user didn't provide a `default_factory` keyword argument, return.
    let Some(keyword) = call.arguments.find_keyword("default_factory") else {
        return;
    };

    // If the value is definitively not callable, return.
    if is_non_callable_value(&keyword.value) {
        return;
    }

    let mut diagnostic = Diagnostic::new(
        DefaultFactoryKwarg {
            default_factory: SourceCodeSnippet::from_str(checker.locator().slice(keyword)),
        },
        call.range(),
    );
    diagnostic.try_set_fix(|| convert_to_positional(call, keyword, checker.locator()));
    checker.diagnostics.push(diagnostic);
}

/// Returns `true` if a value is definitively not callable (e.g., `1` or `[]`).
fn is_non_callable_value(value: &Expr) -> bool {
    is_constant(value)
        || matches!(value, |Expr::List(_)| Expr::Dict(_)
            | Expr::Set(_)
            | Expr::Tuple(_)
            | Expr::Slice(_)
            | Expr::ListComp(_)
            | Expr::SetComp(_)
            | Expr::DictComp(_)
            | Expr::Generator(_)
            | Expr::FString(_))
}

/// Generate an [`Expr`] to replace `defaultdict(default_factory=callable)` with
/// `defaultdict(callable)`.
///
/// For example, given `defaultdict(default_factory=list)`, generate `defaultdict(list)`.
fn convert_to_positional(
    call: &ast::ExprCall,
    default_factory: &Keyword,
    locator: &Locator,
) -> Result<Fix> {
    if call.arguments.len() == 1 {
        // Ex) `defaultdict(default_factory=list)`
        Ok(Fix::unsafe_edit(Edit::range_replacement(
            locator.slice(&default_factory.value).to_string(),
            default_factory.range(),
        )))
    } else {
        // Ex) `defaultdict(member=1, default_factory=list)`

        // First, remove the `default_factory` keyword argument.
        let removal_edit = remove_argument(
            default_factory,
            &call.arguments,
            Parentheses::Preserve,
            locator.contents(),
        )?;

        // Second, insert the value as the first positional argument.
        let insertion_edit = Edit::insertion(
            format!("{}, ", locator.slice(&default_factory.value)),
            call.arguments
                .arguments_source_order()
                .next()
                .ok_or_else(|| anyhow::anyhow!("`default_factory` keyword argument not found"))?
                .start(),
        );

        Ok(Fix::unsafe_edits(insertion_edit, [removal_edit]))
    }
}
