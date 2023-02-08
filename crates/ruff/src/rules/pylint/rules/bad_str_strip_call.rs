use std::fmt;

use ruff_macros::{define_violation, derive_message_formats};
use rustc_hash::FxHashSet;
use rustpython_parser::ast::{Constant, Expr, ExprKind};
use serde::{Deserialize, Serialize};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::settings::types::PythonVersion;
use crate::violation::Violation;

define_violation!(
    pub struct BadStrStripCall {
        strip: StripKind,
        removal: Option<RemovalKind>,
    }
);
impl Violation for BadStrStripCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        let Self { strip, removal } = self;
        if let Some(removal) = removal {
            format!(
                "String `{strip}` call contains duplicate characters (did you mean `{removal}`?)",
            )
        } else {
            format!("String `{strip}` call contains duplicate characters")
        }
    }
}

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

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RemovalKind {
    RemovePrefix,
    RemoveSuffix,
}

impl RemovalKind {
    pub fn for_strip(s: &StripKind) -> Option<Self> {
        match s {
            StripKind::Strip => None,
            StripKind::LStrip => Some(Self::RemovePrefix),
            StripKind::RStrip => Some(Self::RemoveSuffix),
        }
    }
}

impl fmt::Display for RemovalKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let representation = match self {
            Self::RemovePrefix => "removeprefix",
            Self::RemoveSuffix => "removesuffix",
        };
        write!(f, "{representation}")
    }
}

/// Return `true` if a string contains duplicate characters, taking into account
/// escapes.
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
            if let Some(strip) = StripKind::from_str(attr.as_str()) {
                if let Some(arg) = args.get(0) {
                    if let ExprKind::Constant {
                        value: Constant::Str(value),
                        ..
                    } = &arg.node
                    {
                        if has_duplicates(value) {
                            let removal = if checker.settings.target_version >= PythonVersion::Py39
                            {
                                RemovalKind::for_strip(&strip)
                            } else {
                                None
                            };
                            checker.diagnostics.push(Diagnostic::new(
                                BadStrStripCall { strip, removal },
                                Range::from_located(arg),
                            ));
                        }
                    }
                }
            }
        }
    }
}
