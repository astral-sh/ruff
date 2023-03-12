use itertools::Itertools;

use ruff_diagnostics::AlwaysAutofixableViolation;
use ruff_macros::{derive_message_formats, violation};

#[derive(Debug, PartialEq, Eq)]
pub struct UnusedCodes {
    pub unknown: Vec<String>,
    pub disabled: Vec<String>,
    pub unmatched: Vec<String>,
}

#[violation]
pub struct UnusedNOQA {
    pub codes: Option<UnusedCodes>,
}

impl AlwaysAutofixableViolation for UnusedNOQA {
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

    fn autofix_title(&self) -> String {
        "Remove unused `noqa` directive".to_string()
    }
}
