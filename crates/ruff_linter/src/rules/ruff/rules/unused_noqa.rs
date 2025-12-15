use itertools::Itertools;

use ruff_macros::{ViolationMetadata, derive_message_formats};

use crate::AlwaysFixableViolation;

#[derive(Debug, PartialEq, Eq, Default)]
pub(crate) struct UnusedCodes {
    pub disabled: Vec<String>,
    pub duplicated: Vec<String>,
    pub unknown: Vec<String>,
    pub unmatched: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum UnusedNOQAKind {
    Noqa,
    Suppression,
}

impl UnusedNOQAKind {
    fn as_str(&self) -> &str {
        match self {
            UnusedNOQAKind::Noqa => "`noqa` directive",
            UnusedNOQAKind::Suppression => "suppression",
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
/// ## Options
/// - `lint.external`
///
/// ## References
/// - [Ruff error suppression](https://docs.astral.sh/ruff/linter/#error-suppression)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.155")]
pub(crate) struct UnusedNOQA {
    pub codes: Option<UnusedCodes>,
    pub kind: UnusedNOQAKind,
}

impl AlwaysFixableViolation for UnusedNOQA {
    #[derive_message_formats]
    fn message(&self) -> String {
        match &self.codes {
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
                    format!("Unused {}", self.kind.as_str())
                } else {
                    format!(
                        "Unused {} ({})",
                        self.kind.as_str(),
                        codes_by_reason.join("; ")
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
