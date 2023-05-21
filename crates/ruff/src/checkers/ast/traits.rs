use ruff_diagnostics::Diagnostic;

use crate::checkers::ast::RuleContext;
use crate::registry::Rule;

pub(crate) struct RegisteredAstRule<T> {
    pub(crate) run: AstRuleExecutor<T>,
    pub(crate) rule: Rule,
}

// A nice thing about this is that we can have state that lives in this struct,
// and we can pass it to the `run` function... E.g., flake8_bugbear_seen.
impl<T> RegisteredAstRule<T> {
    pub(crate) fn new<R: AstRule<T> + 'static>(rule: Rule) -> Self {
        Self { run: R::run, rule }
    }
}

pub(crate) type AstRuleExecutor<T> =
    fn(diagnostics: &mut Vec<Diagnostic>, checker: &RuleContext, node: &T);

pub(crate) trait AstRule<T>: Sized {
    fn run(diagnostics: &mut Vec<Diagnostic>, checker: &RuleContext, node: &T);
}
