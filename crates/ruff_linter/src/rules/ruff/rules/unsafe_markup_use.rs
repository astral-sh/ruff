use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, ViolationMetadata};

/// ## Removed
/// This rule was implemented in `bandit` and has been remapped to
/// [S704](unsafe-markup-use.md)
///
/// ## What it does
/// Checks for non-literal strings being passed to [`markupsafe.Markup`][markupsafe-markup].
///
/// ## Why is this bad?
/// [`markupsafe.Markup`][markupsafe-markup] does not perform any escaping,
/// so passing dynamic content, like f-strings, variables or interpolated strings
/// will potentially lead to XSS vulnerabilities.
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
pub(crate) struct RuffUnsafeMarkupUse {
    name: String,
}

impl Violation for RuffUnsafeMarkupUse {
    #[derive_message_formats]
    fn message(&self) -> String {
        let RuffUnsafeMarkupUse { name } = self;
        format!("Unsafe use of `{name}` detected")
    }
}
