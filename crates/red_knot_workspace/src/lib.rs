use red_knot_python_semantic::lint::{LintRegistry, LintRegistryBuilder};
use red_knot_python_semantic::register_lints;

pub mod db;
pub mod watch;
pub mod workspace;

pub static DEFAULT_LINT_REGISTRY: std::sync::LazyLock<LintRegistry> =
    std::sync::LazyLock::new(default_lints_registry);

pub fn default_lints_registry() -> LintRegistry {
    let mut builder = LintRegistryBuilder::default();
    register_lints(&mut builder);
    builder.build()
}
