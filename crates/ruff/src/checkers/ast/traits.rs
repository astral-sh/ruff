use ruff_diagnostics::Diagnostic;

use crate::checkers::ast::RuleContext;

pub(crate) struct RegisteredAstRule<T> {
    pub(crate) run: AstRuleExecutor<T>,
}

impl<T> RegisteredAstRule<T> {
    pub(crate) fn new<R: AstRule<T> + 'static>() -> Self {
        Self { run: R::run }
    }
}

pub(crate) type AstRuleExecutor<T> =
    fn(diagnostics: &mut Vec<Diagnostic>, checker: &RuleContext, node: &T);

pub(crate) trait AstRule<T>: Sized {
    fn run(diagnostics: &mut Vec<Diagnostic>, checker: &RuleContext, node: &T);
}
