use itertools::Itertools;

use ruff_diagnostics::AlwaysFixableViolation;
use ruff_macros::{derive_message_formats, violation};

#[derive(Debug, PartialEq, Eq)]
pub struct UnusedCodes {
    pub disabled: Vec<String>,
    pub duplicated: Vec<String>,
    pub unknown: Vec<String>,
    pub unmatched: Vec<String>,
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
/// ## Options
/// - `lint.external`
///
/// ## References
/// - [Ruff error suppression](https://docs.astral.sh/ruff/linter/#error-suppression)
#[violation]
pub struct UnusedNOQA {
    pub codes: Option<UnusedCodes>,
}

impl AlwaysFixableViolation for UnusedNOQA {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnusedNOQA { codes } = self;
        match codes {
            Some(codes) => {
                let mut codes_by_reason = vec![];
                if !codes.unmatched.is_empty() {
                    codes_by_reason.push(format!(
                        "unused: {}",
                        codes
                            .unmatched
                            .iter()
                            .map(|code| format!("`{code}`"))
                            .join(", ")
                    ));
                }
                if !codes.disabled.is_empty() {
                    codes_by_reason.push(format!(
                        "non-enabled: {}",
                        codes
                            .disabled
                            .iter()
                            .map(|code| format!("`{code}`"))
                            .join(", ")
                    ));
                }
                if !codes.duplicated.is_empty() {
                    codes_by_reason.push(format!(
                        "duplicated: {}",
                        codes
                            .duplicated
                            .iter()
                            .map(|code| format!("`{code}`"))
                            .join(", ")
                    ));
                }
                if !codes.unknown.is_empty() {
                    codes_by_reason.push(format!(
                        "unknown: {}",
                        codes
                            .unknown
                            .iter()
                            .map(|code| format!("`{code}`"))
                            .join(", ")
                    ));
                }
                if codes_by_reason.is_empty() {
                    format!("Unused `noqa` directive")
                } else {
                    format!("Unused `noqa` directive ({})", codes_by_reason.join("; "))
                }
            }
            None => format!("Unused blanket `noqa` directive"),
        }
    }

    fn fix_title(&self) -> String {
        "Remove unused `noqa` directive".to_string()
    }
}
