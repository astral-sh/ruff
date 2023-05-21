use rustpython_parser::ast::{self, Ranged};

use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::traits::Analyzer;
use crate::checkers::ast::RuleContext;
use crate::registry::{AsRule, Rule};

#[violation]
pub struct OpenAlias;

impl Violation for OpenAlias {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use builtin `open`")
    }

    fn autofix_title(&self) -> Option<String> {
        Some("Replace with builtin `open`".to_string())
    }
}

impl Analyzer<ast::ExprCall> for OpenAlias {
    fn rule() -> Rule {
        Rule::OpenAlias
    }

    fn run(diagnostics: &mut Vec<Diagnostic>, context: &RuleContext, node: &ast::ExprCall) {
        open_alias(diagnostics, context, node);
    }
}

/// UP020
pub(crate) fn open_alias(
    diagnostics: &mut Vec<Diagnostic>,
    context: &RuleContext,
    ast::ExprCall { func, range, .. }: &ast::ExprCall,
) {
    if context
        .ctx
        .resolve_call_path(func)
        .map_or(false, |call_path| call_path.as_slice() == ["io", "open"])
    {
        let fixable = context
            .ctx
            .find_binding("open")
            .map_or(true, |binding| binding.kind.is_builtin());
        let mut diagnostic = Diagnostic::new(OpenAlias, *range);
        if fixable && context.patch(diagnostic.kind.rule()) {
            #[allow(deprecated)]
            diagnostic.set_fix(Fix::unspecified(Edit::range_replacement(
                "open".to_string(),
                func.range(),
            )));
        }
        diagnostics.push(diagnostic);
    }
}
