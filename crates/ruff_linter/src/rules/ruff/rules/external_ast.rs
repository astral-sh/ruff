use ruff_macros::{CacheKey, ViolationMetadata, derive_message_formats};

/// Diagnostics surfaced by external AST linters
///
/// ## What it does
///
/// This is a meta rule that represents any/all external rules implemented
/// in Python. See more at TODO documentation link
///
/// ## Why is this bad?
///
/// Depends on the rule.
///
#[derive(Debug, Clone, PartialEq, Eq, CacheKey, ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.0")]
pub(crate) struct ExternalLinter {
    pub rule_name: String,
    pub message: String,
}

impl ExternalLinter {
    #[allow(dead_code)]
    pub(crate) fn new(rule_name: impl Into<String>, message: String) -> Self {
        Self {
            rule_name: rule_name.into(),
            message,
        }
    }
}

impl crate::Violation for ExternalLinter {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ExternalLinter { rule_name, message } = self;
        format!("{rule_name}: {message}")
    }
}
