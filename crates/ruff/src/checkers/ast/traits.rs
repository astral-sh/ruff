use ruff_diagnostics::Diagnostic;
use rustpython_parser::ast;

use crate::checkers::ast::ImmutableChecker;

pub(crate) struct RegistryRule {
    pub(crate) run: RuleExecutor,
}

type RuleExecutor =
    fn(diagnostics: &mut Vec<Diagnostic>, checker: &ImmutableChecker, node: &ast::ExprCall);

impl RegistryRule {
    pub(crate) fn new<R: AnalysisRule + 'static>() -> Self {
        Self { run: R::run }
    }

    pub fn run(
        &self,
        diagnostics: &mut Vec<Diagnostic>,
        checker: &ImmutableChecker,
        node: &ast::ExprCall,
    ) {
        (self.run)(diagnostics, checker, node)
    }
}

pub(crate) trait AnalysisRule: Sized {
    fn run(diagnostics: &mut Vec<Diagnostic>, checker: &ImmutableChecker, node: &ast::ExprCall);
}
