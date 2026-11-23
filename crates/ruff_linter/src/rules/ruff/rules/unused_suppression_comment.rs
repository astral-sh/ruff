use itertools::Itertools;

use ruff_macros::{ViolationMetadata, derive_message_formats};

use crate::AlwaysFixableViolation;
use crate::codes::Category;

#[derive(Debug, PartialEq, Eq, Default)]
pub(crate) struct UnusedRules<'a> {
    pub(crate) disabled: &'a [&'a str],
    pub(crate) duplicated: &'a [&'a str],
    pub(crate) unmatched: &'a [&'a str],
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum UnusedSuppressionKind {
    Noqa,
    Suppression,
}

impl UnusedSuppressionKind {
    fn as_str(&self) -> &str {
        match self {
            UnusedSuppressionKind::Noqa => "`noqa` directive",
            UnusedSuppressionKind::Suppression => "suppression",
        }
    }
}

/// ## What it does
/// Checks for `noqa` directives that are no longer applicable.
///
/// ## Why is this bad?
/// A `noqa` directive that no longer matches any diagnostic violations is
/// likely included by mistake, and should be removed to avoid confusion.
///
/// ## Example
/// ```python
/// import foo  # noqa: F401
///
///
/// def bar():
///     foo.bar()
/// ```
///
/// Use instead:
/// ```python
/// import foo
///
///
/// def bar():
///     foo.bar()
/// ```
///
/// ## Conflict with other linters
/// When using `RUF100` with the `--fix` option, Ruff may remove trailing comments
/// that follow a `# noqa` directive on the same line, as it interprets the
/// remainder of the line as a description for the suppression.
///
/// To prevent Ruff from removing suppressions for other tools (like `pylint`
/// or `mypy`), separate them with a second `#` character:
///
/// ```python
/// # Bad: Ruff --fix will remove the pylint comment
/// def visit_ImportFrom(self, node):  # noqa: N802, pylint: disable=invalid-name
///     pass
///
///
/// # Good: Ruff will preserve the pylint comment
/// def visit_ImportFrom(self, node):  # noqa: N802 # pylint: disable=invalid-name
///     pass
/// ```
///
/// ## Fix safety
///
/// The rule's fix is marked as unsafe when a full suppression comment would be removed and there
/// are other nested comments on the same line. Removing such a comment can change the behavior of
/// other suppression comments before or after the removed comment.
///
/// ## See also
///
/// This rule ignores any codes that are unknown to Ruff, as it can't determine
/// if the codes are valid or used by other tools. Enable
/// [`unknown-rule-in-suppression-comment`][RUF102] to flag any unknown rule names or codes.
///
/// ## References
/// - [Ruff error suppression](https://docs.astral.sh/ruff/linter/#error-suppression)
///
/// [RUF102]: https://docs.astral.sh/ruff/rules/unknown-rule-in-suppression-comment/
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.155", category = Category::Suspicious)]
pub(crate) struct UnusedSuppressionComment<'a> {
    pub rules: Option<UnusedRules<'a>>,
    pub kind: UnusedSuppressionKind,
}

impl AlwaysFixableViolation for UnusedSuppressionComment<'_> {
    #[derive_message_formats]
    fn message(&self) -> String {
        match &self.rules {
            Some(rules) => {
                let mut rules_by_reason = vec![];
                if !rules.unmatched.is_empty() {
                    rules_by_reason.push(format!(
                        "unused: {}",
                        rules
                            .unmatched
                            .iter()
                            .map(|rule| format!("`{rule}`"))
                            .join(", ")
                    ));
                }
                if !rules.disabled.is_empty() {
                    rules_by_reason.push(format!(
                        "non-enabled: {}",
                        rules
                            .disabled
                            .iter()
                            .map(|rule| format!("`{rule}`"))
                            .join(", ")
                    ));
                }
                if !rules.duplicated.is_empty() {
                    rules_by_reason.push(format!(
                        "duplicated: {}",
                        rules
                            .duplicated
                            .iter()
                            .map(|rule| format!("`{rule}`"))
                            .join(", ")
                    ));
                }
                if rules_by_reason.is_empty() {
                    format!("Unused {}", self.kind.as_str())
                } else {
                    format!(
                        "Unused {} ({})",
                        self.kind.as_str(),
                        rules_by_reason.join("; ")
                    )
                }
            }
            None => format!("Unused blanket {}", self.kind.as_str()),
        }
    }

    fn fix_title(&self) -> String {
        format!("Remove unused {}", self.kind.as_str())
    }
}
