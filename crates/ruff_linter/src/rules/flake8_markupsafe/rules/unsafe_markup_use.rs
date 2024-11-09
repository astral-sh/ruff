use ruff_python_ast::ExprCall;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::name::QualifiedName;
use ruff_python_semantic::Modules;
use ruff_text_size::Ranged;

use crate::{checkers::ast::Checker, settings::LinterSettings};

/// ## What it does
/// Checks for non-literal strings being passed to [`markupsafe.Markup`].
///
/// ## Why is this bad?
/// `markupsafe.Markup` does not perform any escaping, so passing dynamic
/// content, like f-strings, variables or interpolated strings will potentially
/// lead to XSS vulnerabilities.
///
/// Instead you should interpolate the [`markupsafe.Markup`] object.
///
/// In contrast to the original rule, we do not make an exception for i18n calls
/// within [`markupsafe.Markup`].
///
/// Using [`lint.flake8-markupsafe.extend-markup-names`] additional objects
/// can be treated like [`markupsafe.Markup`].
///
/// ## Example
/// ```python
/// from markupsafe import Markup
///
/// content = "<script>alert('Hello, world!')</script>"
/// html = Markup(f"<b>{content}</b>")  # XSS
/// ```
///
/// Use instead:
/// ```python
/// from markupsafe import Markup
///
/// content = "<script>alert('Hello, world!')</script>"
/// html = Markup("<b>{}</b>").format(content)  # Safe
/// ```
/// ## Options
/// - `lint.flake8-markupsafe.extend-markup-names`
///
/// ## References
/// - [MarkupSafe](https://pypi.org/project/MarkupSafe/)
/// - [`markupsafe.Markup`](https://markupsafe.palletsprojects.com/en/stable/escaping/#markupsafe.Markup)
///
/// [markupsafe.Markup]: https://markupsafe.palletsprojects.com/en/stable/escaping/#markupsafe.Markup
#[violation]
pub struct UnsafeMarkupUse {
    name: String,
}

impl Violation for UnsafeMarkupUse {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnsafeMarkupUse { name } = self;
        format!("Unsafe use of `{name}` detected")
    }
}

/// Checks for unsafe calls to `[markupsafe.Markup]`.
///
/// [markupsafe.Markup]: https://markupsafe.palletsprojects.com/en/stable/escaping/#markupsafe.Markup
pub(crate) fn unsafe_markup_call(checker: &mut Checker, call: &ExprCall) {
    if checker
        .settings
        .flake8_markupsafe
        .extend_markup_names
        .is_empty()
        && !(checker.semantic().seen_module(Modules::MARKUPSAFE)
            || checker.semantic().seen_module(Modules::FLASK))
    {
        return;
    }

    if !is_unsafe_call(call) {
        return;
    }

    let Some(qualified_name) = checker.semantic().resolve_qualified_name(&call.func) else {
        return;
    };

    if !is_markup_call(&qualified_name, checker.settings) {
        return;
    }

    checker.diagnostics.push(Diagnostic::new(
        UnsafeMarkupUse {
            name: qualified_name.to_string(),
        },
        call.range(),
    ));
}

fn is_markup_call(qualified_name: &QualifiedName, settings: &LinterSettings) -> bool {
    matches!(
        qualified_name.segments(),
        ["markupsafe" | "flask", "Markup"]
    ) || settings
        .flake8_markupsafe
        .extend_markup_names
        .iter()
        .map(|target| QualifiedName::from_dotted_name(target))
        .any(|target| *qualified_name == target)
}

fn is_unsafe_call(call: &ExprCall) -> bool {
    // technically this could be circumvented by using a keyword argument
    // but without type-inference we can't really know which keyword argument
    // corresponds to the first positional argument and either way it is
    // unlikely that someone will actually use a keyword argument here
    // TODO: Eventually we may want to allow dynamic values, as long as they
    //       have a __html__ attribute, since that is part of the API
    !(call.arguments.args.is_empty()
        || call.arguments.args[0].is_string_literal_expr()
        || call.arguments.args[0].is_bytes_literal_expr())
}
