use std::fmt;

use crate::checkers::ast::Checker;
use crate::external::ast::registry::ExternalLintRegistry;
use ruff_python_ast::{Expr, Stmt};

#[cfg(not(feature = "ext-lint"))]
mod fallback {
    use std::sync::Arc;

    use super::Checker;
    use crate::external::ExternalLinterError;
    use crate::external::ast::registry::ExternalLintRegistry;
    use ruff_python_ast::{Expr, Stmt};

    #[derive(Clone, Debug)]
    pub(crate) struct ExternalLintRuntime {
        registry: Arc<ExternalLintRegistry>,
    }

    impl ExternalLintRuntime {
        pub(crate) fn new(registry: ExternalLintRegistry) -> Self {
            Self {
                registry: Arc::new(registry),
            }
        }

        pub(crate) fn registry(&self) -> &ExternalLintRegistry {
            &self.registry
        }

        #[allow(clippy::unused_self)]
        pub(crate) fn run_on_stmt(&self, _checker: &Checker<'_>, _stmt: &Stmt) {}

        #[allow(clippy::unused_self)]
        pub(crate) fn run_on_expr(&self, _checker: &Checker<'_>, _expr: &Expr) {}

        #[allow(clippy::unused_self)]
        pub(crate) fn run_in_session<F, R>(&self, f: F) -> R
        where
            F: FnOnce() -> R,
        {
            f()
        }
    }

    pub fn verify_registry_scripts(
        _registry: &ExternalLintRegistry,
    ) -> Result<(), ExternalLinterError> {
        Ok(())
    }
}

#[cfg(feature = "ext-lint")]
mod python;

#[cfg(not(feature = "ext-lint"))]
use fallback as imp;
#[cfg(feature = "ext-lint")]
use python as imp;

pub(crate) use imp::ExternalLintRuntime;
pub use imp::verify_registry_scripts;

#[derive(Clone)]
pub struct ExternalLintRuntimeHandle {
    runtime: ExternalLintRuntime,
}

#[cfg_attr(not(feature = "ext-lint"), allow(dead_code))]
impl ExternalLintRuntimeHandle {
    pub fn new(registry: ExternalLintRegistry) -> Self {
        Self {
            runtime: ExternalLintRuntime::new(registry),
        }
    }

    pub fn registry(&self) -> &ExternalLintRegistry {
        self.runtime.registry()
    }

    #[cfg_attr(not(feature = "ext-lint"), allow(dead_code))]
    pub(crate) fn run_on_stmt(&self, checker: &Checker<'_>, stmt: &Stmt) {
        self.runtime.run_on_stmt(checker, stmt);
    }

    #[cfg_attr(not(feature = "ext-lint"), allow(dead_code))]
    pub(crate) fn run_on_expr(&self, checker: &Checker<'_>, expr: &Expr) {
        self.runtime.run_on_expr(checker, expr);
    }

    pub(crate) fn run_in_session<F, R>(&self, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        self.runtime.run_in_session(f)
    }
}

impl fmt::Debug for ExternalLintRuntimeHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut debug = f.debug_struct("ExternalLintRuntimeHandle");
        debug.field("registry", self.runtime.registry());
        debug.field("runtime", &self.runtime);
        debug.finish()
    }
}
