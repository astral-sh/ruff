use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::TextRange;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for banned imports at module level. The banned imports are allowed inline, such as
/// within a function definition or an `if TYPE_CHECKING:` block.
///
/// ## Why is this bad?
/// Some modules take a relatively long time to import, such as `torch` or `tensorflow`. Library
/// authors might want to ensure that you only pay the import cost for these modules if you
/// directly use them, rather than if you import a module that happens to use an expensive module
/// in one of its functions.
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
        format!("`{name}` is banned at module level. Please move the import inline.")
    }
}

enum NameMatchPolicy {
    ExactOnly,
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
