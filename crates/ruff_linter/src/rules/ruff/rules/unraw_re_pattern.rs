use crate::checkers::ast::Checker;
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{Expr, ExprBytesLiteral, ExprCall, ExprStringLiteral};
use ruff_python_semantic::{Modules, SemanticModel};
use ruff_text_size::{Ranged, TextRange};
use std::fmt::{Display, Formatter};
use std::str::FromStr;

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
/// re.compile("foo\\bar")
/// ```
///
/// Use instead:
///
/// ```python
/// re.compile(r"foo\bar")
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

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum RegexModule {
    Re,
    Regex,
}

impl RegexModule {
    fn is_regex(&self) -> bool {
        matches!(self, RegexModule::Regex)
    }

    fn is_function_taking_pattern(&self, name: &str) -> bool {
        match name {
            "compile" | "findall" | "finditer" | "fullmatch" | "match" | "search" | "split"
            | "sub" | "subn" => true,
            "splititer" | "subf" | "subfn" | "template" => self.is_regex(),
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

    let func = func.to_string();
    let diagnostic = Diagnostic::new(UnrawRePattern { module, func, kind }, range);

    checker.diagnostics.push(diagnostic);
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
