mod general;
mod structured;

use super::DocstringFragment;

/// Returns the display label for an explicit reStructuredText role.
///
/// For example, `("py:meth", "~list.pop")` becomes `"pop"` and
/// `("class", "Widget <package.Widget>")` becomes `"Widget"`.
fn rest_role_display_label<'a>(name: &str, markup: &'a str) -> &'a str {
    let explicit_title = markup
        .strip_suffix('>')
        .and_then(|markup| markup.split_once('<'))
        .map(|(title, _)| title.trim_end());

    explicit_title.unwrap_or_else(|| interpreted_text_label(markup, is_python_domain_role(name)))
}

/// Returns whether `name` is a Sphinx Python-domain cross-reference role.
fn is_python_domain_role(name: &str) -> bool {
    let mut components = name.rsplit(':');
    let Some(role) = components.next() else {
        return false;
    };

    matches!(components.next(), None | Some("py"))
        && matches!(
            role,
            "attr"
                | "class"
                | "const"
                | "data"
                | "deco"
                | "exc"
                | "func"
                | "meth"
                | "mod"
                | "obj"
                | "type"
        )
}

/// Returns the display label for reStructuredText interpreted text.
///
/// For example, `"~pkg.Widget"` becomes `"Widget"`; a Python role target like `".lines.line"`
/// becomes `"lines.line"`.
fn interpreted_text_label(text: &str, is_python_role_target: bool) -> &str {
    let (abbreviated, target) = text
        .strip_prefix('~')
        .map_or((false, text), |target| (true, target));
    let target = if is_python_role_target {
        target.strip_prefix('.').unwrap_or(target)
    } else {
        target
    };
    if target.is_empty() {
        return text;
    }

    if abbreviated {
        target.rsplit_once('.').map_or(target, |(_, label)| label)
    } else {
        target
    }
}

/// Render Markdown for a source docstring.
///
/// `source` must have already undergone PEP-257 trimming and universal newline
/// normalization (typically via `docstring::documentation_trim`).
pub(super) fn render(source: &str) -> String {
    let mut output = String::new();
    structured::render_into(&mut output, source);
    output
}

impl DocstringFragment {
    pub(super) fn render_markdown(&self) -> String {
        let mut output = String::new();
        general::render_fragment_into(&mut output, &self.0);
        output
    }
}
