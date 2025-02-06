use std::fmt;

use ruff_python_ast::{self as ast, Expr};
use rustc_hash::FxHashSet;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
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
#[derive(ViolationMetadata)]
pub(crate) struct BadStrStripCall {
    duplicated: char,
    strip: StripKind,
    removal: Option<RemovalKind>,
}

impl Violation for BadStrStripCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        let Self {
            duplicated,
            strip,
            removal,
        } = self;

        if let Some(removal) = removal {
            format!(
                "String `{strip}` call contains duplicate character {duplicated:#?} \
                (did you mean `{removal}`?)",
            )
        } else {
            format!("String `{strip}` call contains duplicate character {duplicated:#?}")
        }
    }
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub(crate) enum ValueKind {
    String,
    Bytes,
}

impl ValueKind {
    fn from(expr: &Expr) -> Option<Self> {
        match expr {
            Expr::StringLiteral(_) => Some(Self::String),
            Expr::BytesLiteral(_) => Some(Self::Bytes),
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

fn find_duplicate_in_string(string: &ast::StringLiteralValue) -> Option<char> {
    find_duplicate(string.chars())
}

fn find_duplicate_in_bytes(bytes: &ast::BytesLiteralValue) -> Option<char> {
    find_duplicate(bytes.bytes().map(char::from))
}

/// Return `true` if a string or byte sequence contains duplicate characters.
fn find_duplicate(mut chars: impl Iterator<Item = char>) -> Option<char> {
    let mut seen = FxHashSet::default();

    chars.find(|&char| !seen.insert(char))
}

/// PLE1310
pub(crate) fn bad_str_strip_call(checker: &Checker, call: &ast::ExprCall) {
    if checker.settings.target_version < PythonVersion::Py39 {
        return;
    }

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

    let Some(value_kind) = ValueKind::from(value.as_ref()) else {
        return;
    };

    let duplicated = match arg {
        Expr::StringLiteral(string) if value_kind == ValueKind::String => {
            find_duplicate_in_string(&string.value)
        }
        Expr::BytesLiteral(bytes) if value_kind == ValueKind::Bytes => {
            find_duplicate_in_bytes(&bytes.value)
        }
        _ => return,
    };

    let Some(duplicated) = duplicated else {
        return;
    };
    let removal = RemovalKind::for_strip(strip);

    let diagnostic = Diagnostic::new(
        BadStrStripCall {
            duplicated,
            strip,
            removal,
        },
        arg.range(),
    );

    checker.report_diagnostic(diagnostic);
}
