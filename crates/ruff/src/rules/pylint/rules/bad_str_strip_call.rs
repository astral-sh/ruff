use std::fmt;

use rustc_hash::FxHashSet;
use rustpython_ast::{Constant, Expr, ExprKind};
use rustpython_parser::lexer;
use rustpython_parser::lexer::Tok;
use serde::{Deserialize, Serialize};

use ruff_macros::derive_message_formats;

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::define_violation;
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::rules::pydocstyle::helpers::{leading_quote, trailing_quote};
use crate::violation::AlwaysAutofixableViolation;

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StripKind {
    Strip,
    LStrip,
    RStrip,
}

impl StripKind {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "strip" => Some(Self::Strip),
            "lstrip" => Some(Self::LStrip),
            "rstrip" => Some(Self::RStrip),
            _ => None,
        }
    }
}

impl fmt::Display for StripKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let representation = match self {
            Self::Strip => "strip",
            Self::LStrip => "lstrip",
            Self::RStrip => "rstrip",
        };
        write!(f, "{representation}")
    }
}

define_violation!(
    pub struct BadStrStripCall {
        kind: StripKind,
    }
);
impl AlwaysAutofixableViolation for BadStrStripCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        let Self { kind } = self;
        format!("String `{kind}` call contains duplicate characters")
    }

    fn autofix_title(&self) -> String {
        "Remove duplicate characters".to_string()
    }
}

/// Remove duplicate characters from an escaped string.
fn deduplicate_escaped(s: &str) -> String {
    let mut result = String::new();
    let mut escaped = false;
    let mut seen = FxHashSet::default();
    for ch in s.chars() {
        if escaped {
            escaped = false;
            let pair = format!("\\{}", ch);
            if !seen.insert(pair) {
                continue;
            }
        } else if ch == '\\' {
            escaped = true;
        } else if !seen.insert(ch.to_string()) {
            continue;
        }
        result.push(ch);
    }
    result
}

/// Remove duplicate characters from a raw string.
fn deduplicate_raw(s: &str) -> String {
    let mut result = String::new();
    let mut seen = FxHashSet::default();
    for ch in s.chars() {
        if !seen.insert(ch) {
            continue;
        }
        result.push(ch);
    }
    result
}

/// Return `true` if a string contains duplicate characters, taking into account escapes.
fn has_duplicates(s: &str) -> bool {
    let mut escaped = false;
    let mut seen = FxHashSet::default();
    for ch in s.chars() {
        if escaped {
            escaped = false;
            let pair = format!("\\{}", ch);
            if !seen.insert(pair) {
                return true;
            }
        } else if ch == '\\' {
            escaped = true;
        } else if !seen.insert(ch.to_string()) {
            return true;
        }
    }
    false
}

/// PLE1310
pub fn bad_str_strip_call(checker: &mut Checker, func: &Expr, args: &[Expr]) {
    if let ExprKind::Attribute { value, attr, .. } = &func.node {
        if matches!(
            value.node,
            ExprKind::Constant {
                value: Constant::Str(_) | Constant::Bytes(_),
                ..
            }
        ) {
            if let Some(kind) = StripKind::from_str(attr.as_str()) {
                if let Some(arg) = args.get(0) {
                    if let ExprKind::Constant {
                        value: Constant::Str(value),
                        ..
                    } = &arg.node
                    {
                        let is_multiline = arg.location.row() != arg.end_location.unwrap().row();

                        let module_text = checker
                            .locator
                            .slice_source_code_range(&Range::from_located(arg));

                        if !is_multiline
                            && lexer::make_tokenizer_located(module_text, arg.location)
                                .flatten()
                                .filter(|(_, tok, _)| matches!(tok, Tok::String { .. }))
                                .nth(1)
                                .is_none()
                        {
                            // If we have a single string (no implicit concatenation), fix it.
                            let Some(leading_quote) = leading_quote(module_text) else {
                                return;
                            };
                            let Some(trailing_quote) = trailing_quote(module_text) else {
                                return;
                            };
                            let content = &module_text
                                [leading_quote.len()..module_text.len() - trailing_quote.len()];

                            let deduplicated =
                                if leading_quote.contains('r') || leading_quote.contains('R') {
                                    deduplicate_raw(content)
                                } else {
                                    deduplicate_escaped(content)
                                };
                            if content != deduplicated {
                                let mut diagnostic = Diagnostic::new(
                                    BadStrStripCall { kind },
                                    Range::from_located(arg),
                                );
                                if checker.patch(diagnostic.kind.rule()) {
                                    diagnostic.amend(Fix::replacement(
                                        format!("{leading_quote}{deduplicated}{trailing_quote}"),
                                        arg.location,
                                        arg.end_location.unwrap(),
                                    ));
                                };
                                checker.diagnostics.push(diagnostic);
                            }
                        } else {
                            // Otherwise, let's just look for duplicates.
                            if has_duplicates(value) {
                                checker.diagnostics.push(Diagnostic::new(
                                    BadStrStripCall { kind },
                                    Range::from_located(arg),
                                ));
                            }
                        }
                    }
                }
            }
        }
    }
}
