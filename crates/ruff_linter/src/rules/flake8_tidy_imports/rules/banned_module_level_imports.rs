use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::rules::flake8_tidy_imports::matchers::NameMatchPolicy;

/// ## What it does
/// Checks for module-level imports that should instead be imported lazily
/// (e.g., within a function definition, or an `if TYPE_CHECKING:` block, or
/// some other nested context).
///
/// ## Why is this bad?
/// Some modules are expensive to import. For example, importing `torch` or
/// `tensorflow` can introduce a noticeable delay in the startup time of a
/// Python program.
///
/// In such cases, you may want to enforce that the module is imported lazily
/// as needed, rather than at the top of the file. This could involve inlining
/// the import into the function that uses it, rather than importing it
/// unconditionally, to ensure that the module is only imported when necessary.
///
/// ## Example
/// ```python
/// import tensorflow as tf
///
///
/// def show_version():
///     print(tf.__version__)
/// ```
///
/// Use instead:
/// ```python
/// def show_version():
///     import tensorflow as tf
///
///     print(tf.__version__)
/// ```
///
/// ## Options
/// - `lint.flake8-tidy-imports.banned-module-level-imports`
#[violation]
pub struct BannedModuleLevelImports {
    name: String,
}

impl Violation for BannedModuleLevelImports {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BannedModuleLevelImports { name } = self;
        format!("`{name}` is banned at the module level")
    }
}

/// TID253
pub(crate) fn banned_module_level_imports<T: Ranged>(
    checker: &mut Checker,
    policy: &NameMatchPolicy,
    node: &T,
) {
    if !checker.semantic().at_top_level() {
        return;
    }

    if let Some(banned_module) = policy.find(
        checker
            .settings
            .flake8_tidy_imports
            .banned_module_level_imports
            .iter()
            .map(AsRef::as_ref),
    ) {
        checker.diagnostics.push(Diagnostic::new(
            BannedModuleLevelImports {
                name: banned_module,
            },
            node.range(),
        ));
    }
}
