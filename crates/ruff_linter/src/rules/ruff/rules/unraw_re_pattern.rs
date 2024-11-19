use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{Expr, ExprBytesLiteral, ExprCall, ExprStringLiteral};
use ruff_python_semantic::{Modules, SemanticModel};
use ruff_text_size::{Ranged, TextRange};
use std::fmt::{Display, Formatter};

use crate::checkers::ast::Checker;

/// ## What it does
/// Reports the following `re` and `regex` calls when
/// their first arguments are not raw strings:
///
/// - Both modules: `compile`, `findall`, `finditer`,
///   `fullmatch`, `match`, `search`, `split`, `sub`, `subn`.
/// - `regex`-specific: `splititer`, `subf`, `subfn`, `template`.
///
/// ## Why is this bad?
/// Regular expressions should be written
/// using raw strings to avoid double escaping.
///
/// ## Example
///
/// ```python
/// re.compile('foo\\bar')
/// ```
///
/// Use instead:
/// ```python
/// re.compile(r'foo\bar')
/// ```
#[violation]
pub struct UnrawRePattern {
    module: RegexModule,
    func: String,
    kind: PatternKind,
}

impl Violation for UnrawRePattern {
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

#[derive(Debug, Eq, PartialEq)]
enum RegexModule {
    Re,
    Regex,
}

impl RegexModule {
    fn is_regex(&self) -> bool {
        matches!(self, RegexModule::Regex)
    }
}

impl Display for RegexModule {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                RegexModule::Re => "re",
                RegexModule::Regex => "regex",
            }
        )
    }
}

#[derive(Debug, Eq, PartialEq)]
enum PatternKind {
    String,
    Bytes,
}

/// RUF051
pub(crate) fn unraw_re_pattern(checker: &mut Checker, call: &ExprCall) {
    let semantic = checker.semantic();

    if !semantic.seen_module(Modules::RE) && !semantic.seen_module(Modules::REGEX) {
        return;
    }

    let Some((module, func)) = regex_module_and_func(semantic, call.func.as_ref()) else {
        return;
    };
    let Some((kind, range)) = pattern_kind_and_range(call.arguments.args.as_ref()) else {
        return;
    };

    let diagnostic = Diagnostic::new(UnrawRePattern { module, func, kind }, range);

    checker.diagnostics.push(diagnostic);
}

fn regex_module_and_func(semantic: &SemanticModel, expr: &Expr) -> Option<(RegexModule, String)> {
    let qualified_name = semantic.resolve_qualified_name(expr)?;

    let (module, func) = match qualified_name.segments() {
        [module, func] => match *module {
            "re" => (RegexModule::Re, func),
            "regex" => (RegexModule::Regex, func),
            _ => return None,
        },
        _ => return None,
    };

    if is_shared(func) || module.is_regex() && is_regex_specific(func) {
        return Some((module, func.to_string()));
    }

    None
}

fn pattern_kind_and_range(arguments: &[Expr]) -> Option<(PatternKind, TextRange)> {
    let first = arguments.first()?;
    let range = first.range();

    let pattern_kind = match first {
        Expr::StringLiteral(ExprStringLiteral { value, .. }) => {
            if value.is_implicit_concatenated() || value.is_raw() {
                return None;
            }

            PatternKind::String
        }

        Expr::BytesLiteral(ExprBytesLiteral { value, .. }) => {
            if value.is_implicit_concatenated() || value.is_raw() {
                return None;
            }

            PatternKind::Bytes
        }

        _ => return None,
    };

    Some((pattern_kind, range))
}

/// Whether `func` is an attribute of both `re` and `regex`.
fn is_shared(func: &str) -> bool {
    matches!(
        func,
        "compile"
            | "findall"
            | "finditer"
            | "fullmatch"
            | "match"
            | "search"
            | "split"
            | "sub"
            | "subn"
    )
}

/// Whether `func` is an extension specific to `regex`.
fn is_regex_specific(func: &str) -> bool {
    matches!(func, "splititer" | "subf" | "subfn" | "template")
}
