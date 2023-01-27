use once_cell::sync::Lazy;
use regex::Regex;
use ruff_macros::derive_message_formats;
use rustpython_ast::Location;

use crate::ast::types::Range;
use crate::define_violation;
use crate::fix::Fix;
use crate::registry::{Diagnostic, DiagnosticKind, Rule};
use crate::settings::Settings;
use crate::violation::AlwaysAutofixableViolation;

static EXTRANEOUS_WHITESPACE_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"([\[({][ \t]|[ \t][\]}),;:])([^=]|$)").unwrap());

define_violation!(
    pub struct WhitespaceAfterBrace {
        pub brace: String,
    }
);
impl AlwaysAutofixableViolation for WhitespaceAfterBrace {
    #[derive_message_formats]
    fn message(&self) -> String {
        let WhitespaceAfterBrace { brace } = self;
        format!("Whitespace after `{brace}`")
    }

    fn autofix_title(&self) -> String {
        let WhitespaceAfterBrace { brace } = self;
        format!("Remove whitespace after `{brace}`")
    }
}

define_violation!(
    pub struct WhitespaceBeforeBrace {
        pub brace: String,
    }
);
impl AlwaysAutofixableViolation for WhitespaceBeforeBrace {
    #[derive_message_formats]
    fn message(&self) -> String {
        let WhitespaceBeforeBrace { brace } = self;
        format!("Whitespace before `{brace}`")
    }

    fn autofix_title(&self) -> String {
        let WhitespaceBeforeBrace { brace } = self;
        format!("Remove whitespace before `{brace}`")
    }
}

define_violation!(
    pub struct WhitespaceBeforeCommaSemicolonColon {
        pub value: String,
    }
);
impl AlwaysAutofixableViolation for WhitespaceBeforeCommaSemicolonColon {
    #[derive_message_formats]
    fn message(&self) -> String {
        let WhitespaceBeforeCommaSemicolonColon { value } = self;
        format!("Whitespace before `{value}`")
    }

    fn autofix_title(&self) -> String {
        let WhitespaceBeforeCommaSemicolonColon { value } = self;
        format!("Remove whitespace before `{value}`")
    }
}

/// E201, E202, E203
pub fn check_whitespace(
    lineno: usize,
    line: &str,
    autofix: bool,
    settings: &Settings,
) -> Vec<Diagnostic> {
    let mut diagnostics: Vec<Diagnostic> = Vec::new();

    for caps in EXTRANEOUS_WHITESPACE_REGEX.captures_iter(line) {
        if let Some(m) = &caps.get(1) {
            let start_location = Location::new(lineno + 1, m.start());
            let end_location = Location::new(lineno + 1, m.end());

            let (violation, should_fix): (DiagnosticKind, bool) = match m.as_str().trim() {
                brace @ ("(" | "{" | "[")
                    if settings.rules.enabled(&Rule::WhitespaceAfterBrace) =>
                {
                    (
                        WhitespaceAfterBrace {
                            brace: brace.to_string(),
                        }
                        .into(),
                        settings.rules.should_fix(&Rule::WhitespaceAfterBrace),
                    )
                }
                brace @ (")" | "}" | "]")
                    if settings.rules.enabled(&Rule::WhitespaceBeforeBrace) =>
                {
                    (
                        WhitespaceBeforeBrace {
                            brace: brace.to_string(),
                        }
                        .into(),
                        settings.rules.should_fix(&Rule::WhitespaceBeforeBrace),
                    )
                }
                value @ ("," | ";" | ":")
                    if settings
                        .rules
                        .enabled(&Rule::WhitespaceBeforeCommaSemicolonColon) =>
                {
                    (
                        WhitespaceBeforeCommaSemicolonColon {
                            value: value.to_string(),
                        }
                        .into(),
                        settings
                            .rules
                            .should_fix(&Rule::WhitespaceBeforeCommaSemicolonColon),
                    )
                }
                _ => continue,
            };

            let mut diagnostic =
                Diagnostic::new(violation, Range::new(start_location, end_location));

            if autofix && should_fix {
                diagnostic.amend(Fix::replacement(
                    format!("{}", &caps[1].trim()),
                    start_location,
                    end_location,
                ));
            }
            diagnostics.push(diagnostic);
        }
    }
    diagnostics
}
