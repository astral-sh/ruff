use ruff_python_ast::{Expr, ExprCall};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::name::QualifiedName;
use ruff_python_semantic::{Modules, SemanticModel};
use ruff_text_size::Ranged;

use crate::{checkers::ast::Checker, settings::LinterSettings};

/// ## What it does
/// Checks for non-literal strings being passed to [`markupsafe.Markup`][markupsafe-markup].
///
/// ## Why is this bad?
/// [`markupsafe.Markup`] does not perform any escaping, so passing dynamic
/// content, like f-strings, variables or interpolated strings will potentially
/// lead to XSS vulnerabilities.
///
/// Instead you should interpolate the `Markup` object.
///
/// Using [`lint.flake8-bandit.extend-markup-names`] additional objects can be
/// treated like `Markup`.
///
/// This rule was originally inspired by [flake8-markupsafe] but doesn't carve
/// out any exceptions for i18n related calls by default.
///
/// You can use [`lint.flake8-bandit.allowed-markup-calls`] to specify exceptions.
///
/// ## Example
/// Given:
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
///
/// Given:
/// ```python
/// from markupsafe import Markup
///
/// lines = [
///     Markup("<b>heading</b>"),
///     "<script>alert('XSS attempt')</script>",
/// ]
/// html = Markup("<br>".join(lines))  # XSS
/// ```
///
/// Use instead:
/// ```python
/// from markupsafe import Markup
///
/// lines = [
///     Markup("<b>heading</b>"),
///     "<script>alert('XSS attempt')</script>",
/// ]
/// html = Markup("<br>").join(lines)  # Safe
/// ```
/// ## Options
/// - `lint.flake8-bandit.extend-markup-names`
/// - `lint.flake8-bandit.allowed-markup-calls`
///
/// ## References
/// - [MarkupSafe on PyPI](https://pypi.org/project/MarkupSafe/)
/// - [`markupsafe.Markup` API documentation](https://markupsafe.palletsprojects.com/en/stable/escaping/#markupsafe.Markup)
///
/// [markupsafe-markup]: https://markupsafe.palletsprojects.com/en/stable/escaping/#markupsafe.Markup
/// [flake8-markupsafe]: https://github.com/vmagamedov/flake8-markupsafe
#[derive(ViolationMetadata)]
pub(crate) struct UnsafeMarkupUse {
    name: String,
}

impl Violation for UnsafeMarkupUse {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnsafeMarkupUse { name } = self;
        format!("Unsafe use of `{name}` detected")
    }
}

/// S704
pub(crate) fn unsafe_markup_call(checker: &Checker, call: &ExprCall) {
    if checker
        .settings
        .flake8_bandit
        .extend_markup_names
        .is_empty()
        && !(checker.semantic().seen_module(Modules::MARKUPSAFE)
            || checker.semantic().seen_module(Modules::FLASK))
    {
        return;
    }

    if !is_unsafe_call(call, checker.semantic(), checker.settings) {
        return;
    }

    let Some(qualified_name) = checker.semantic().resolve_qualified_name(&call.func) else {
        return;
    };

    if !is_markup_call(&qualified_name, checker.settings) {
        return;
    }

    checker.report_diagnostic(Diagnostic::new(
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
        .flake8_bandit
        .extend_markup_names
        .iter()
        .map(|target| QualifiedName::from_dotted_name(target))
        .any(|target| *qualified_name == target)
}

fn is_unsafe_call(call: &ExprCall, semantic: &SemanticModel, settings: &LinterSettings) -> bool {
    // technically this could be circumvented by using a keyword argument
    // but without type-inference we can't really know which keyword argument
    // corresponds to the first positional argument and either way it is
    // unlikely that someone will actually use a keyword argument here
    // TODO: Eventually we may want to allow dynamic values, as long as they
    //       have a __html__ attribute, since that is part of the API
    matches!(&*call.arguments.args, [first] if !first.is_string_literal_expr() && !first.is_bytes_literal_expr() && !is_whitelisted_call(first, semantic, settings))
}

fn is_whitelisted_call(expr: &Expr, semantic: &SemanticModel, settings: &LinterSettings) -> bool {
    let Expr::Call(ExprCall { func, .. }) = expr else {
        return false;
    };

    let Some(qualified_name) = semantic.resolve_qualified_name(func) else {
        return false;
    };

    settings
        .flake8_bandit
        .allowed_markup_calls
        .iter()
        .map(|target| QualifiedName::from_dotted_name(target))
        .any(|target| qualified_name == target)
}
