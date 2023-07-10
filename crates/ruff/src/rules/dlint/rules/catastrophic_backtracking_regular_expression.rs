use regex_syntax::hir::{Hir, HirKind, Repetition};
use regex_syntax::ParserBuilder;
use ruff_diagnostics::{Diagnostic, Violation};
use rustpython_parser::ast;
use rustpython_parser::ast::{Constant, Expr, ExprCall, Ranged};

use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::call_path::CallPath;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for regular expressions that can, under certain inputs, lead to catastrophic
/// backtracking in the Python `re` module.
///
/// ## Why is this bad?
/// Catastrophic backtracking will often lead to denial-of-service. Catastrophic cases may take
/// days, weeks, or years to complete which may leave your service degraded or unusable.
///
/// ## Example
/// ```python
/// import re
///
/// subject = 'a' * 64
/// re.search(r'(.|[abc])+z', subject)  # Boom
/// ```
///
/// Use instead:
/// ```python
/// import re
///
/// subject = 'a' * 64
/// re.search(r'.+z', subject)
/// ```
///
/// ## References
/// - [Runaway Regular Expressions: Catastrophic Backtracking](https://www.regular-expressions.info/catastrophic.html)
/// - [Preventing Regular Expression Denial of Service (ReDoS)](https://www.regular-expressions.info/redos.html)
#[violation]
pub struct CatastrophicBacktrackingRegularExpression;

impl Violation for CatastrophicBacktrackingRegularExpression {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Potentially dangerous regex expression can lead to catastrophic backtracking")
    }
}

/// DUO138
pub(crate) fn catastrophic_backtracking_regular_expression(checker: &mut Checker, call: &ExprCall) {
    // Check if function id is a regex function first so we do not do an unnecessary expensive call
    // to resolve_call_path()
    if let Expr::Attribute(ast::ExprAttribute { attr, .. }) = &call.func.as_ref() {
        if ![
            "compile",
            "search",
            "match",
            "fullmatch",
            "split",
            "findall",
            "finditer",
            "sub",
            "subn",
        ]
        .contains(&attr.as_str())
        {
            return;
        }
    }

    if let Some(call_path) = checker.semantic().resolve_call_path(call.func.as_ref()) {
        if !is_regex_func(&call_path) {
            return;
        }
    }

    // Regex string should always be the first argument
    let Expr::Constant(ast::ExprConstant {value: Constant::Str(regex_string), .. }) = &call.args[0] else {
        return;
    };

    // Create HIR from regex string
    let mut parser = ParserBuilder::new().build();
    let hir = parser.parse(regex_string).unwrap();

    if detect_redos(&hir) {
        checker
            .diagnostics
            .push(Diagnostic::new(CatastrophicBacktrackingRegularExpression, call.func.range()));
    }
}

fn is_regex_func(call_path: &CallPath) -> bool {
    matches!(
        call_path.as_slice(),
        [
            "re",
            "compile"
                | "search"
                | "match"
                | "fullmatch"
                | "split"
                | "findall"
                | "finditer"
                | "sub"
                | "subn"
        ]
    )
}

const MAX_REPETITION_COUNT: u32 = 10;

fn detect_redos(hir: &Hir) -> bool {
    // Vulnerable regex patterns come in two scenarios. The regex must have grouping with repetition
    // Then inside the repeated group one of two things can lead to catastrophic backtracking
    //   1. Repetition
    //   2. Alternation with overlapping
    match hir.kind() {
        HirKind::Concat(hirs) | HirKind::Alternation(hirs) => hirs.iter().any(detect_redos),
        // If there is an unbounded repetition inside of a repetition this could lead to
        // catastrophic backtracking
        HirKind::Repetition(outer_repetition) => {
            let inner_repetition = match outer_repetition.sub.kind() {
                HirKind::Repetition(inner) => Some(inner),
                HirKind::Capture(capture) => capture.sub.kind().as_repetition(),
                _ => None,
            };

            if let Some(inner_repetition) = inner_repetition {
                if inner_repetition.greedy && (inner_repetition.max.is_none() || inner_repetition.max.unwrap() > MAX_REPETITION_COUNT) {
                    return true;
                }
            }

            false
        },
        _ => false,
    }
}

trait HirKindExt {
    fn as_repetition(&self) -> Option<&Repetition>;
}

impl HirKindExt for HirKind {
    fn as_repetition(&self) -> Option<&Repetition> {
        match self {
            HirKind::Repetition(repetition) => Some(repetition),
            _ => None,
        }
    }
}
