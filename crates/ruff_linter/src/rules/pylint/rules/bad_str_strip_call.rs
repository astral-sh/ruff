use std::fmt;

use ruff_python_ast::{self as ast, Expr};
use rustc_hash::FxHashSet;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::settings::types::PythonVersion;

/// ## What it does
/// Checks duplicate characters in `str.strip` calls.
///
/// ## Why is this bad?
/// All characters in `str.strip` calls are removed from both the leading and
/// trailing ends of the string. Including duplicate characters in the call
/// is redundant and often indicative of a mistake.
///
/// In Python 3.9 and later, you can use `str.removeprefix` and
/// `str.removesuffix` to remove an exact prefix or suffix from a string,
/// respectively, which should be preferred when possible.
///
/// ## Example
/// ```python
/// # Evaluates to "foo".
/// "bar foo baz".strip("bar baz ")
/// ```
///
/// Use instead:
/// ```python
/// # Evaluates to "foo".
/// "bar foo baz".strip("abrz ")  # "foo"
/// ```
///
/// Or:
/// ```python
/// # Evaluates to "foo".
/// "bar foo baz".removeprefix("bar ").removesuffix(" baz")
/// ```
///
/// ## Options
/// - `target-version`
///
/// ## References
/// - [Python documentation: `str.strip`](https://docs.python.org/3/library/stdtypes.html?highlight=strip#str.strip)
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
fn has_duplicates(s: &ast::StringLiteralValue) -> bool {
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
    if let Expr::Attribute(ast::ExprAttribute { value, attr, .. }) = func {
        if matches!(
            value.as_ref(),
            Expr::StringLiteral(_) | Expr::BytesLiteral(_)
        ) {
            if let Some(strip) = StripKind::from_str(attr.as_str()) {
                if let Some(arg) = args.first() {
                    if let Expr::StringLiteral(ast::ExprStringLiteral { value, .. }) = &arg {
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
