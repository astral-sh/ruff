use ruff_text_size::{TextLen, TextRange};
use rustpython_parser::ast::{self, Ranged};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::find_keyword;

use crate::checkers::ast::traits::AstAnalyzer;
use crate::checkers::ast::RuleContext;
use crate::registry::{AsRule, Rule};

#[violation]
pub struct ReplaceUniversalNewlines;

impl AlwaysAutofixableViolation for ReplaceUniversalNewlines {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`universal_newlines` is deprecated, use `text`")
    }

    fn autofix_title(&self) -> String {
        "Replace with `text` keyword argument".to_string()
    }
}

impl AstAnalyzer<ast::ExprCall> for ReplaceUniversalNewlines {
    fn rule() -> Rule {
        Rule::ReplaceUniversalNewlines
    }

    fn run(diagnostics: &mut Vec<Diagnostic>, checker: &RuleContext, node: &ast::ExprCall) {
        replace_universal_newlines(diagnostics, checker, node);
    }
}

/// UP021
pub(crate) fn replace_universal_newlines(
    diagnostics: &mut Vec<Diagnostic>,
    checker: &RuleContext,
    ast::ExprCall { func, keywords, .. }: &ast::ExprCall,
) {
    if checker
        .ctx
        .resolve_call_path(func)
        .map_or(false, |call_path| {
            call_path.as_slice() == ["subprocess", "run"]
        })
    {
        let Some(keyword) = find_keyword(keywords, "universal_newlines") else { return; };
        let range = TextRange::at(keyword.start(), "universal_newlines".text_len());
        let mut diagnostic = Diagnostic::new(ReplaceUniversalNewlines, range);
        if checker.patch(diagnostic.kind.rule()) {
            #[allow(deprecated)]
            diagnostic.set_fix(Fix::unspecified(Edit::range_replacement(
                "text".to_string(),
                range,
            )));
        }
        diagnostics.push(diagnostic);
    }
}
