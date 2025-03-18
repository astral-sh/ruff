use std::fmt::{Display, Formatter};
use std::str::FromStr;

use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{
    BytesLiteral, Expr, ExprBytesLiteral, ExprCall, ExprStringLiteral, StringLiteral,
};
use ruff_python_semantic::{Modules, SemanticModel};

use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

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
    let mut diagnostic = Diagnostic::new(UnrawRePattern { module, func, kind }, range);

    if
    // The (no-op) `u` prefix is a syntax error when combined with `r`
    !literal.flags.prefix().is_unicode()
    // We are looking for backslash characters
    // in the raw source code here, because `\n`
    // gets converted to a single character already
    // at the lexing stage.
    &&!checker.locator().slice(literal.range()).contains('\\')
    {
        diagnostic.set_fix(Fix::safe_edit(Edit::insertion(
            "r".to_string(),
            literal.range().start(),
        )));
    }
    checker.report_diagnostic(diagnostic);
}

fn check_bytes(checker: &Checker, literal: &BytesLiteral, module: RegexModule, func: &str) {
    if literal.flags.prefix().is_raw() {
        return;
    }

    let kind = PatternKind::Bytes;
    let func = func.to_string();
    let range = literal.range;
    let diagnostic = Diagnostic::new(UnrawRePattern { module, func, kind }, range);

    checker.report_diagnostic(diagnostic);
}
