use ruff_diagnostics::Diagnostic;

use crate::checkers::ast::RuleContext;
use crate::registry::Rule;
use crate::settings::Settings;

/// Trait for a lint rule that can be run on an AST node of type `T`.
pub(crate) trait Analyzer<T>: Sized {
    /// The [`Rule`] that this analyzer implements.
    fn rule() -> Rule;

    /// Run the analyzer on the given node.
    fn run(diagnostics: &mut Vec<Diagnostic>, checker: &RuleContext, node: &T);
}

/// Internal representation of a single [`Rule`] that can be run on an AST node of type `T`.
pub(super) struct RegisteredRule<T> {
    rule: Rule,
    run: Executor<T>,
}

impl<T> RegisteredRule<T> {
    pub(super) fn new<R: Analyzer<T> + 'static>() -> Self {
        Self {
            rule: R::rule(),
            run: R::run,
        }
    }

    #[inline]
    pub(super) fn enabled(&self, settings: &Settings) -> bool {
        settings.rules.enabled(self.rule)
    }

    #[inline]
    pub(super) fn run(&self, diagnostics: &mut Vec<Diagnostic>, context: &RuleContext, node: &T) {
        (self.run)(diagnostics, context, node);
    }
}

/// Executor for an [`Analyzer`] as a generic function pointer.
type Executor<T> = fn(diagnostics: &mut Vec<Diagnostic>, checker: &RuleContext, node: &T);
