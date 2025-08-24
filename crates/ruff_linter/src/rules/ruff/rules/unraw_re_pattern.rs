use std::fmt::{Display, Formatter};
use std::str::FromStr;

use ruff_diagnostics::Applicability;
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{
    BytesLiteral, Expr, ExprBytesLiteral, ExprCall, ExprStringLiteral, PythonVersion, StringLiteral,
};
use ruff_python_semantic::{Modules, SemanticModel};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::{Edit, Fix, FixAvailability, Violation};

/// ## What it does
/// Reports the following `re` and `regex` calls when
/// their first arguments are not raw strings:
///
/// - For `regex` and `re`: `compile`, `findall`, `finditer`,
///   `fullmatch`, `match`, `search`, `split`, `sub`, `subn`.
/// - `regex`-specific: `splititer`, `subf`, `subfn`, `template`.
///
/// ## Why is this bad?
/// Regular expressions should be written
/// using raw strings to avoid double escaping.
///
/// ## Fix safety
/// The fix is unsafe if the string/bytes literal contains an escape sequence because the fix alters
/// the runtime value of the literal while retaining the regex semantics.
///
/// For example
/// ```python
/// # Literal is `1\n2`.
/// re.compile("1\n2")
///
/// # Literal is `1\\n2`, but the regex library will interpret `\\n` and will still match a newline
/// # character as before.
/// re.compile(r"1\n2")
/// ```
///
/// ## Fix availability
///  A fix is not available if either
///  * the argument is a string with a (no-op) `u` prefix (e.g., `u"foo"`) as the prefix is
///    incompatible with the raw prefix `r`
///  * the argument is a string or bytes literal with an escape sequence that has a different
///    meaning in the context of a regular expression such as `\b`, which is word boundary or
///    backspace in a regex, depending on the context, but always a backspace in string and bytes
///    literals.
///
/// ## Example
///
/// ```python
/// re.compile("foo\\bar")
/// ```
///
/// Use instead:
///
/// ```python
/// re.compile(r"foo\bar")
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct UnrawRePattern {
    module: RegexModule,
    func: String,
    kind: PatternKind,
}

impl Violation for UnrawRePattern {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;
    #[derive_message_formats]
    fn message(&self) -> String {
        let Self { module, func, kind } = &self;
        let call = format!("`{module}.{func}()`");

        match kind {
            PatternKind::String => format!("First argument to {call} is not raw string"),
            PatternKind::Bytes => format!("First argument to {call} is not raw bytes literal"),
        }
    }

    fn fix_title(&self) -> Option<String> {
        match self.kind {
            PatternKind::String => Some("Replace with raw string".to_string()),
            PatternKind::Bytes => Some("Replace with raw bytes literal".to_string()),
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum RegexModule {
    Re,
    Regex,
}

impl RegexModule {
    fn is_function_taking_pattern(self, name: &str) -> bool {
        match name {
            "compile" | "findall" | "finditer" | "fullmatch" | "match" | "search" | "split"
            | "sub" | "subn" => true,
            "splititer" | "subf" | "subfn" | "template" => self == Self::Regex,
            _ => false,
        }
    }
}

impl Display for RegexModule {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            RegexModule::Re => "re",
            RegexModule::Regex => "regex",
        })
    }
}

impl FromStr for RegexModule {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "re" => Ok(Self::Re),
            "regex" => Ok(Self::Regex),
            _ => Err(()),
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum PatternKind {
    String,
    Bytes,
}

/// RUF039
pub(crate) fn unraw_re_pattern(checker: &Checker, call: &ExprCall) {
    let semantic = checker.semantic();

    if !semantic.seen_module(Modules::RE) && !semantic.seen_module(Modules::REGEX) {
        return;
    }

    let Some((module, func)) = regex_module_and_func(semantic, call.func.as_ref()) else {
        return;
    };

    match call.arguments.args.as_ref().first() {
        Some(Expr::StringLiteral(ExprStringLiteral { value, .. })) => {
            value
                .iter()
                .for_each(|part| check_string(checker, part, module, func));
        }
        Some(Expr::BytesLiteral(ExprBytesLiteral { value, .. })) => {
            value
                .iter()
                .for_each(|part| check_bytes(checker, part, module, func));
        }
        _ => {}
    }
}

fn regex_module_and_func<'model>(
    semantic: &SemanticModel<'model>,
    expr: &'model Expr,
) -> Option<(RegexModule, &'model str)> {
    let qualified_name = semantic.resolve_qualified_name(expr)?;

    if let [module, func] = qualified_name.segments() {
        let module = RegexModule::from_str(module).ok()?;

        if !module.is_function_taking_pattern(func) {
            return None;
        }

        return Some((module, func));
    }

    None
}

fn check_string(checker: &Checker, literal: &StringLiteral, module: RegexModule, func: &str) {
    if literal.flags.prefix().is_raw() {
        return;
    }

    let kind = PatternKind::String;
    let func = func.to_string();
    let range = literal.range;
    let mut diagnostic = checker.report_diagnostic(UnrawRePattern { module, func, kind }, range);

    let Some(applicability) = raw_string_applicability(checker, literal) else {
        return;
    };

    diagnostic.set_fix(Fix::applicable_edit(
        Edit::insertion("r".to_string(), literal.range().start()),
        applicability,
    ));
}

/// Check how safe it is to prepend the `r` prefix to the string.
///
/// ## Returns
///  * `None` if the prefix cannot be added,
///  * `Some(a)` if it can be added with applicability `a`.
fn raw_string_applicability(checker: &Checker, literal: &StringLiteral) -> Option<Applicability> {
    if literal.flags.prefix().is_unicode() {
        // The (no-op) `u` prefix is a syntax error when combined with `r`
        return None;
    }

    if checker.target_version() >= PythonVersion::PY38 {
        raw_applicability(checker, literal.range(), |escaped| {
            matches!(
                escaped,
                Some('a' | 'f' | 'n' | 'r' | 't' | 'u' | 'U' | 'v' | 'x' | 'N')
            )
        })
    } else {
        raw_applicability(checker, literal.range(), |escaped| {
            matches!(
                escaped,
                Some('a' | 'f' | 'n' | 'r' | 't' | 'u' | 'U' | 'v' | 'x')
            )
        })
    }

    // re.compile("\a\f\n\N{Partial Differential}\r\t\u27F2\U0001F0A1\v\x41")  # with unsafe fix
}

fn check_bytes(checker: &Checker, literal: &BytesLiteral, module: RegexModule, func: &str) {
    if literal.flags.prefix().is_raw() {
        return;
    }

    let kind = PatternKind::Bytes;
    let func = func.to_string();
    let range = literal.range;
    let mut diagnostic = checker.report_diagnostic(UnrawRePattern { module, func, kind }, range);

    let Some(applicability) = raw_byte_applicability(checker, literal) else {
        return;
    };

    diagnostic.set_fix(Fix::applicable_edit(
        Edit::insertion("r".to_string(), literal.range().start()),
        applicability,
    ));
}

/// Check how same it is to prepend the `r` prefix to the byte sting.
///
/// ## Returns
///  * `None` if the prefix cannot be added,
///  * `Some(a)` if it can be added with applicability `a`.
fn raw_byte_applicability(checker: &Checker, literal: &BytesLiteral) -> Option<Applicability> {
    raw_applicability(checker, literal.range(), |escaped| {
        matches!(escaped, Some('a' | 'f' | 'n' | 'r' | 't' | 'v' | 'x'))
    })
}

fn raw_applicability(
    checker: &Checker,
    literal_range: TextRange,
    match_allowed_escape_sequence: impl Fn(Option<char>) -> bool,
) -> Option<Applicability> {
    let mut found_slash = false;
    let mut chars = checker.locator().slice(literal_range).chars().peekable();
    while let Some(char) = chars.next() {
        if char == '\\' {
            found_slash = true;
            // Turning `"\uXXXX"` into `r"\uXXXX"` is behaviorally equivalent when passed
            // to `re`, however, it's not exactly the same runtime value.
            // Similarly, for the other escape sequences.
            if !match_allowed_escape_sequence(chars.peek().copied()) {
                // If the next character is not one of the whitelisted ones, we likely cannot safely turn
                // this into a raw string.
                return None;
            }
        }
    }

    Some(if found_slash {
        Applicability::Unsafe
    } else {
        Applicability::Safe
    })
}
