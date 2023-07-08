use regex_syntax::hir::{Hir, HirKind};
use regex_syntax::ParserBuilder;
use ruff_diagnostics::{Diagnostic, Violation};
use rustpython_parser::ast;
use rustpython_parser::ast::{Constant, Expr, ExprCall, Ranged};
use similar::DiffableStr;

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
pub struct CatastrophicReUse;

impl Violation for CatastrophicReUse {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Potentially dangerous regex expression can lead to catastrophic backtracking")
    }
}

/// DUO138
pub(crate) fn catastrophic_re_use(checker: &mut Checker, call: &ExprCall) {
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
            .push(Diagnostic::new(CatastrophicReUse, call.func.range()));
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

fn detect_redos(hir: &Hir) -> bool {
    match hir.kind() {
        HirKind::Concat(hirs) | HirKind::Alternation(hirs) => {
            hirs.iter().any(|sub_hir| detect_redos(sub_hir))
        }
        HirKind::Repetition(repetition) => repetition.greedy,
        _ => false,
    }
}
