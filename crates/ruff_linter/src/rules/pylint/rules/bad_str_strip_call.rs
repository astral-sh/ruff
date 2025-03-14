use std::fmt;

use ruff_python_ast::{self as ast, Expr};
use rustc_hash::FxHashSet;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_semantic::analyze::typing;
use ruff_python_semantic::SemanticModel;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use ruff_python_ast::PythonVersion;

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
#[derive(ViolationMetadata)]
pub(crate) struct BadStrStripCall {
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
pub(crate) enum ValueKind {
    String,
    Bytes,
}

impl ValueKind {
    fn from(expr: &Expr, semantic: &SemanticModel) -> Option<Self> {
        match expr {
            Expr::StringLiteral(_) => Some(Self::String),
            Expr::BytesLiteral(_) => Some(Self::Bytes),
            Expr::Name(name) => {
                let binding_id = semantic.only_binding(name)?;
                let binding = semantic.binding(binding_id);

                if typing::is_string(binding, semantic) {
                    Some(Self::String)
                } else if typing::is_bytes(binding, semantic) {
                    Some(Self::Bytes)
                } else {
                    None
                }
            }
            _ => None,
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

fn string_has_duplicate_char(string: &ast::StringLiteralValue) -> bool {
    has_duplicate(string.chars())
}

fn bytes_has_duplicate_char(bytes: &ast::BytesLiteralValue) -> bool {
    has_duplicate(bytes.bytes().map(char::from))
}

/// Return true if a string or byte sequence has a duplicate.
fn has_duplicate(mut chars: impl Iterator<Item = char>) -> bool {
    let mut seen = FxHashSet::default();

    chars.any(|char| !seen.insert(char))
}

/// PLE1310
pub(crate) fn bad_str_strip_call(checker: &Checker, call: &ast::ExprCall) {
    let (func, arguments) = (&call.func, &call.arguments);

    let Expr::Attribute(ast::ExprAttribute { value, attr, .. }) = func.as_ref() else {
        return;
    };

    let Some(strip) = StripKind::from_str(attr.as_str()) else {
        return;
    };

    if !arguments.keywords.is_empty() {
        return;
    }

    let [arg] = arguments.args.as_ref() else {
        return;
    };

    let value = &**value;

    let Some(value_kind) = ValueKind::from(value, checker.semantic()) else {
        return;
    };

    let duplicated = match arg {
        Expr::StringLiteral(string) if value_kind == ValueKind::String => {
            string_has_duplicate_char(&string.value)
        }
        Expr::BytesLiteral(bytes) if value_kind == ValueKind::Bytes => {
            bytes_has_duplicate_char(&bytes.value)
        }
        _ => return,
    };

    if !duplicated {
        return;
    }

    let removal = if checker.target_version() >= PythonVersion::PY39 {
        RemovalKind::for_strip(strip)
    } else {
        None
    };

    let diagnostic = Diagnostic::new(BadStrStripCall { strip, removal }, arg.range());

    checker.report_diagnostic(diagnostic);
}
