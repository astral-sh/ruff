use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::TextRange;

use crate::checkers::ast::Checker;

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
/// - `flake8-tidy-imports.banned-module-level-imports`
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
pub(crate) fn name_is_banned_at_module_level(
    checker: &mut Checker,
    name: &str,
    text_range: TextRange,
) {
    banned_at_module_level_with_policy(checker, name, text_range, &NameMatchPolicy::ExactOnly);
}

/// TID253
pub(crate) fn name_or_parent_is_banned_at_module_level(
    checker: &mut Checker,
    name: &str,
    text_range: TextRange,
) {
    banned_at_module_level_with_policy(checker, name, text_range, &NameMatchPolicy::ExactOrParents);
}

#[derive(Debug)]
enum NameMatchPolicy {
    /// Only match an exact module name (e.g., given `import foo.bar`, only match `foo.bar`).
    ExactOnly,
    /// Match an exact module name or any of its parents (e.g., given `import foo.bar`, match
    /// `foo.bar` or `foo`).
    ExactOrParents,
}

fn banned_at_module_level_with_policy(
    checker: &mut Checker,
    name: &str,
    text_range: TextRange,
    policy: &NameMatchPolicy,
) {
    if !checker.semantic().at_top_level() {
        return;
    }
    let banned_module_level_imports = &checker
        .settings
        .flake8_tidy_imports
        .banned_module_level_imports;
    for banned_module_name in banned_module_level_imports {
        let name_is_banned = match policy {
            NameMatchPolicy::ExactOnly => name == banned_module_name,
            NameMatchPolicy::ExactOrParents => {
                name == banned_module_name || name.starts_with(&format!("{banned_module_name}."))
            }
        };
        if name_is_banned {
            checker.diagnostics.push(Diagnostic::new(
                BannedModuleLevelImports {
                    name: banned_module_name.to_string(),
                },
                text_range,
            ));
            return;
        }
    }
}
