use ruff_diagnostics::Diagnostic;

use crate::checkers::ast::ImmutableChecker;

pub(crate) struct RegistryRule<T> {
    pub(crate) run: RuleExecutor<T>,
}

type RuleExecutor<T> = fn(diagnostics: &mut Vec<Diagnostic>, checker: &ImmutableChecker, node: &T);

impl<T> RegistryRule<T> {
    pub(crate) fn new<R: AnalysisRule<T> + 'static>() -> Self {
        Self { run: R::run }
    }
}

pub(crate) trait AnalysisRule<T>: Sized {
    fn run(diagnostics: &mut Vec<Diagnostic>, checker: &ImmutableChecker, node: &T);
}
