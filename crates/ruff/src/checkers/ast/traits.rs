use ruff_diagnostics::Diagnostic;

use crate::checkers::ast::RuleContext;
use crate::registry::Rule;
use crate::settings::Settings;

/// Trait for a lint rule that can be run on an AST node of type `T`.
pub(crate) trait AstAnalyzer<T>: Sized {
    /// The [`Rule`] that this analyzer implements.
    fn rule() -> Rule;

    /// Run the analyzer on the given node.
    fn run(diagnostics: &mut Vec<Diagnostic>, checker: &RuleContext, node: &T);
}

/// Internal representation of a single [`Rule`] that can be run on an AST node of type `T`.
pub(super) struct RegisteredAstRule<T> {
    rule: Rule,
    run: Run<T>,
}

impl<T> RegisteredAstRule<T> {
    pub(super) fn new<R: AstAnalyzer<T> + 'static>() -> Self {
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

/// Executor for an [`AstAnalyzer`] as a generic function pointer.
type Run<T> = fn(diagnostics: &mut Vec<Diagnostic>, checker: &RuleContext, node: &T);
