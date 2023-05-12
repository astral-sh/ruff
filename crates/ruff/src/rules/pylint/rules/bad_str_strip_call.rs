use std::fmt;

use rustc_hash::FxHashSet;
use rustpython_parser::ast::{self, Constant, Expr, ExprKind};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;
use crate::settings::types::PythonVersion;

#[violation]
pub struct BadStrStripCall {
    strip: StripKind,
    removal: Option<RemovalKind>,
}

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

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub(crate) enum StripKind {
    Strip,
    LStrip,
    RStrip,
}

impl StripKind {
    pub(crate) fn from_str(s: &str) -> Option<Self> {
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

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub(crate) enum RemovalKind {
    RemovePrefix,
    RemoveSuffix,
}

impl RemovalKind {
    pub(crate) fn for_strip(s: StripKind) -> Option<Self> {
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
            let pair = format!("\\{ch}");
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
pub(crate) fn bad_str_strip_call(checker: &mut Checker, func: &Expr, args: &[Expr]) {
    if let ExprKind::Attribute(ast::ExprAttribute { value, attr, .. }) = &func.node {
        if matches!(
            value.node,
            ExprKind::Constant(ast::ExprConstant {
                value: Constant::Str(_) | Constant::Bytes(_),
                ..
            })
        ) {
            if let Some(strip) = StripKind::from_str(attr.as_str()) {
                if let Some(arg) = args.get(0) {
                    if let ExprKind::Constant(ast::ExprConstant {
                        value: Constant::Str(value),
                        ..
                    }) = &arg.node
                    {
                        if has_duplicates(value) {
                            let removal = if checker.settings.target_version >= PythonVersion::Py39
                            {
                                RemovalKind::for_strip(strip)
                            } else {
                                None
                            };
                            checker.diagnostics.push(Diagnostic::new(
                                BadStrStripCall { strip, removal },
                                arg.range(),
                            ));
                        }
                    }
                }
            }
        }
    }
}
